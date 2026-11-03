use std::sync::Arc;

use asset_game::WeaponSoundSlot;
use assets::PreparedWeapons;
use bevy::prelude::*;
use entity_iw4::EntityEventKind;

use crate::backend::AudioScope;
use crate::sources::{SourceCue, SourceCueRequest};
use crate::{AudioRuntime, SoundBank, WeaponSound};

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct WeaponSoundOutput<'w> {
    weapons: Option<Res<'w, PreparedWeapons>>,
    bank: Option<Res<'w, SoundBank>>,
    sounds: MessageWriter<'w, WeaponSound>,
}

impl WeaponSoundOutput<'_> {
    pub(crate) fn is_available(&self) -> bool {
        self.bank.is_some() && self.weapons.is_some()
    }

    pub(crate) fn write_event(
        &mut self,
        sound: &net::EntityEventSound,
        identity: &net::CEntity,
        player_view: bool,
    ) {
        let event = sound.event.event;
        let weapon = if matches!(
            event,
            EntityEventKind::DETONATE
                | EntityEventKind::ITEM_PICKUP
                | EntityEventKind::AMMO_PICKUP
                | EntityEventKind::PREP_OFFHAND
                | EntityEventKind::USE_OFFHAND
        ) {
            let Ok(weapon) = u32::try_from(sound.event.payload.event_parm) else {
                return;
            };
            weapon
        } else {
            sound.event.payload.weapon
        };
        let Some(weapons) = self.weapons.as_deref() else {
            diag::warn!(
                Audio,
                "audio: entity sound weapon is unavailable (typed gap)"
            );
            return;
        };
        if weapons.registry().sounds_of(weapon).is_none() {
            diag::warn!(
                Audio,
                "audio: entity sound weapon is unavailable (typed gap)"
            );
            return;
        }
        let selected = self
            .bank
            .as_deref()
            .ok_or_else(|| "sound bank is unavailable".to_owned())
            .and_then(|bank| {
                selected_alias(
                    &weapons.registry(),
                    &bank.0,
                    weapon,
                    event,
                    player_view,
                    sound.event.payload.event_parm == 1,
                )
            });
        let (namespace, alias) = match selected {
            Ok(Some(selected)) => selected,
            Ok(None) => return,
            Err(error) => {
                diag::warn!(
                    Audio,
                    "audio: entity sound event {} refused: weapon={} player_view={player_view}: {error}",
                    event.0,
                    weapons.registry().name_of(weapon)
                );
                return;
            }
        };
        self.sounds.write(WeaponSound {
            event: Some(crate::AudioEvent::from_entity(
                sound.entity,
                &sound.event,
                0,
            )),
            namespace,
            alias: alias.to_owned(),
            origin_inches: (!player_view
                || matches!(
                    event,
                    EntityEventKind::MELEE_HIT | EntityEventKind::MELEE_MISS
                ))
            .then_some(sound.event.payload.origin),
            snd_ent: Some(u32::from(identity.number())),
        });
    }

    fn slot(
        &self,
        weapon: u32,
        slots: [WeaponSoundSlot; 2],
    ) -> Option<(asset_core::FamilyId, &str)> {
        let weapons = self.weapons.as_ref()?.registry();
        let bank = &self.bank.as_ref()?.0;
        slots
            .into_iter()
            .find_map(|slot| weapons.weapon_sound_key(weapon, slot, bank))
    }

    pub(crate) fn write_slots(
        &mut self,
        weapon: u32,
        slots: [WeaponSoundSlot; 2],
        origin_inches: Option<[f32; 3]>,
        snd_ent: Option<u32>,
    ) {
        let Some((namespace, alias)) = self.slot(weapon, slots) else {
            return;
        };
        let sound = WeaponSound {
            event: None,
            namespace,
            alias: alias.to_owned(),
            origin_inches,
            snd_ent,
        };
        self.sounds.write(sound);
    }

    pub(crate) fn source_cue(
        &self,
        runtime: &AudioRuntime,
        weapon: u32,
        slots: [WeaponSoundSlot; 2],
        number: u32,
    ) -> Option<Arc<SourceCue>> {
        let (namespace, alias) = self.slot(weapon, slots)?;
        runtime.source_cue(SourceCueRequest {
            bank: self.bank.as_ref()?.0.clone(),
            namespace,
            alias: alias.into(),
            emitter: Some(number),
            scope: AudioScope::Match,
            group: None,
        })
    }
}

fn selected_alias<'a>(
    weapons: &asset_game::WeaponRegistry,
    bank: &'a asset_audio::SoundCatalog,
    weapon: u32,
    event: EntityEventKind,
    player_view: bool,
    knife: bool,
) -> Result<Option<(asset_core::FamilyId, &'a str)>, String> {
    let weapon = if matches!(
        event,
        EntityEventKind::MELEE_SWIPE | EntityEventKind::MELEE_HIT | EntityEventKind::MELEE_MISS
    ) {
        weapons.melee_weapon_of(weapon)
    } else {
        weapon
    };
    let (world, player) = match event {
        EntityEventKind::ITEM_PICKUP => (WeaponSoundSlot::Pickup, WeaponSoundSlot::PickupPlayer),
        EntityEventKind::AMMO_PICKUP => (
            WeaponSoundSlot::AmmoPickup,
            WeaponSoundSlot::AmmoPickupPlayer,
        ),
        EntityEventKind::NOAMMO => (WeaponSoundSlot::EmptyFire, WeaponSoundSlot::EmptyFirePlayer),
        EntityEventKind::RELOAD => (WeaponSoundSlot::Reload, WeaponSoundSlot::ReloadPlayer),
        EntityEventKind::RELOAD_FROM_EMPTY => (
            WeaponSoundSlot::ReloadEmpty,
            WeaponSoundSlot::ReloadEmptyPlayer,
        ),
        EntityEventKind::RELOAD_START => (
            WeaponSoundSlot::ReloadStart,
            WeaponSoundSlot::ReloadStartPlayer,
        ),
        EntityEventKind::RELOAD_END => {
            (WeaponSoundSlot::ReloadEnd, WeaponSoundSlot::ReloadEndPlayer)
        }
        EntityEventKind::RAISE_WEAPON => (WeaponSoundSlot::Raise, WeaponSoundSlot::RaisePlayer),
        EntityEventKind::FIRST_RAISE_WEAPON => (
            WeaponSoundSlot::FirstRaise,
            WeaponSoundSlot::FirstRaisePlayer,
        ),
        EntityEventKind::PUTAWAY_WEAPON => {
            (WeaponSoundSlot::Putaway, WeaponSoundSlot::PutawayPlayer)
        }
        EntityEventKind::WEAPON_ALT => {
            (WeaponSoundSlot::AltSwitch, WeaponSoundSlot::AltSwitchPlayer)
        }
        EntityEventKind::PULLBACK_WEAPON => {
            (WeaponSoundSlot::Pullback, WeaponSoundSlot::PullbackPlayer)
        }
        EntityEventKind::PREP_OFFHAND => {
            (WeaponSoundSlot::Pullback, WeaponSoundSlot::PullbackPlayer)
        }
        EntityEventKind::USE_OFFHAND => (WeaponSoundSlot::Fire, WeaponSoundSlot::FirePlayer),
        EntityEventKind::DETONATE => (WeaponSoundSlot::Detonate, WeaponSoundSlot::DetonatePlayer),
        EntityEventKind::RECHAMBER_WEAPON => {
            (WeaponSoundSlot::Rechamber, WeaponSoundSlot::RechamberPlayer)
        }
        EntityEventKind::MELEE_SWIPE => (
            WeaponSoundSlot::MeleeSwipe,
            WeaponSoundSlot::MeleeSwipePlayer,
        ),
        EntityEventKind::MELEE_HIT => {
            return weapons
                .melee_impact_sound_key(weapon, knife, asset_game::MeleeImpact::Hit, bank)
                .map(Some)
                .ok_or_else(|| "melee hit sound is not in the sound bank".to_owned());
        }
        EntityEventKind::MELEE_MISS => {
            return weapons
                .melee_impact_sound_key(weapon, knife, asset_game::MeleeImpact::Miss, bank)
                .map(Some)
                .ok_or_else(|| "melee miss sound is not in the sound bank".to_owned());
        }
        _ => return Ok(None),
    };

    let slot = if player_view { player } else { world };
    // Weapon sound slots are optional. An empty slot is authored silence;
    // sounds can also come from animation notetracks rather than this event.
    let Some(alias) = weapons.authored_weapon_sound(weapon, slot) else {
        return Ok(None);
    };
    weapons
        .weapon_sound_key(weapon, slot, bank)
        .map(Some)
        .ok_or_else(|| format!("{slot:?} alias `{alias}` is not in the sound bank"))
}
