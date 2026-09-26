use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use assets::{PreparedWeapons, WeaponSoundSlot};
use bevy::prelude::*;
use net::{CEntity, EntityEventKind, LocalPresentClient, PresentedSnapshot};

use crate::{
    Footstep, LandSound, PlayAlias, SoundBank, StepGait, WeaponSound, footstep_aliases,
    gear_rattle_alias, land_aliases, mantle_gear_alias, snd_ent_from_number,
};

fn selected_alias<'a>(
    weapons: &assets::WeaponRegistry,
    bank: &'a assets::SoundCatalog,
    weapon: u32,
    event: EntityEventKind,
    player_view: bool,
) -> Option<(assets::AssetNamespace, &'a str)> {
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
        EntityEventKind::RECHAMBER_WEAPON => {
            (WeaponSoundSlot::Rechamber, WeaponSoundSlot::RechamberPlayer)
        }
        EntityEventKind::MELEE_SWIPE => (
            WeaponSoundSlot::MeleeSwipe,
            WeaponSoundSlot::MeleeSwipePlayer,
        ),
        EntityEventKind::MELEE_HIT => {
            return weapons.weapon_sound_key(weapon, WeaponSoundSlot::MeleeHit, bank);
        }
        EntityEventKind::MELEE_MISS => {
            return weapons.weapon_sound_key(weapon, WeaponSoundSlot::MeleeMiss, bank);
        }
        _ => return None,
    };

    weapons.weapon_sound_key(weapon, if player_view { player } else { world }, bank)
}

fn cg_entity_event_sound(
    sound: On<net::EntityEventSound>,
    identities: Query<&CEntity>,
    local: Res<LocalPresentClient>,
    weapons: Option<Res<PreparedWeapons>>,
    bank: Option<Res<SoundBank>>,
    adopted: Option<Res<net::LastAdoptedSnapshot>>,
    mut output: MessageWriter<WeaponSound>,
    mut play: MessageWriter<crate::AliasCommand>,
) {
    let event = sound.event.event;
    if event == EntityEventKind::SOUND_ALIAS {
        play_cs_sound_alias(&sound.event, adopted.as_deref(), &mut play);
        return;
    }
    if event == EntityEventKind::SOUND_ALIAS_AS_MASTER {
        diag::warn!(
            Audio,
            "audio: EV_SOUND_ALIAS_AS_MASTER is not ported (typed gap)"
        );
        return;
    }
    let Ok(identity) = identities.get(sound.entity) else {
        diag::warn!(
            Audio,
            "audio: entity sound has no CEntity identity (typed gap)"
        );
        return;
    };
    let player_view = identity.client() == Some(local.0);
    if !matches!(
        event,
        EntityEventKind::ITEM_PICKUP
            | EntityEventKind::AMMO_PICKUP
            | EntityEventKind::NOAMMO
            | EntityEventKind::RELOAD
            | EntityEventKind::RELOAD_FROM_EMPTY
            | EntityEventKind::RELOAD_START
            | EntityEventKind::RELOAD_END
            | EntityEventKind::RAISE_WEAPON
            | EntityEventKind::FIRST_RAISE_WEAPON
            | EntityEventKind::PUTAWAY_WEAPON
            | EntityEventKind::WEAPON_ALT
            | EntityEventKind::PULLBACK_WEAPON
            | EntityEventKind::PREP_OFFHAND
            | EntityEventKind::USE_OFFHAND
            | EntityEventKind::RECHAMBER_WEAPON
            | EntityEventKind::MELEE_SWIPE
            | EntityEventKind::MELEE_HIT
            | EntityEventKind::MELEE_MISS
    ) {
        diag::warn!(
            Audio,
            "audio: entity sound event {} has no handler (typed gap)",
            event.0
        );
        return;
    }
    let Some(weapons) = weapons.as_deref() else {
        diag::warn!(
            Audio,
            "audio: entity sound weapon is unavailable (typed gap)"
        );
        return;
    };
    if weapons.0.sounds_of(sound.event.payload.weapon).is_none() {
        diag::warn!(
            Audio,
            "audio: entity sound weapon is unavailable (typed gap)"
        );
        return;
    }
    // Offhand view sounds may be authored entirely as animation notetracks.
    // A missing player alias must not fall back to the world throw sound.
    if event == EntityEventKind::USE_OFFHAND
        && weapons
            .0
            .authored_weapon_sound(
                sound.event.payload.weapon,
                if player_view {
                    WeaponSoundSlot::FirePlayer
                } else {
                    WeaponSoundSlot::Fire
                },
            )
            .is_none()
    {
        return;
    }
    let Some((namespace, alias)) = bank.as_deref().and_then(|bank| {
        selected_alias(
            &weapons.0,
            &bank.0,
            sound.event.payload.weapon,
            event,
            player_view,
        )
    }) else {
        diag::warn!(
            Audio,
            "audio: entity sound event {} has no authored alias",
            event.0
        );
        return;
    };
    output.write(WeaponSound {
        namespace,
        alias: alias.to_owned(),
        origin_inches: (!player_view).then_some(sound.event.payload.origin),
        snd_ent: Some(u32::from(identity.number())),
    });
}

fn play_cs_sound_alias(
    payload: &net::DispatchedEntityEvent,
    adopted: Option<&net::LastAdoptedSnapshot>,
    play: &mut MessageWriter<crate::AliasCommand>,
) {
    let index = u8::try_from(payload.payload.event_parm).unwrap_or(0);
    let Some(alias) = adopted.and_then(|snap| snap.sound_alias_name(index).map(str::to_owned))
    else {
        diag::warn!(
            Audio,
            "audio: EV_SOUND_ALIAS CS index {index} is unresolved (typed gap)"
        );
        return;
    };
    let local = payload.payload.number == net::LOCAL_SOUND_ENTITY;
    diag::event!(
        Audio,
        Debug,
        "cs_sound",
        "audio: cs sound `{alias}` local={local}"
    );
    play.write(crate::AliasCommand::Play(PlayAlias {
        namespace: assets::AssetNamespace::Iw4,
        alias,
        fallback: None,
        origin_inches: (!local).then_some(payload.payload.origin),
        snd_ent: if local {
            None
        } else {
            snd_ent_from_number(payload.payload.number)
        },
    }));
}

fn cg_movement_sound(
    sound: On<net::EntityMovementSound>,
    identities: Query<&CEntity>,
    local: Res<LocalPresentClient>,
    presented: Res<PresentedSnapshot>,
    mut footsteps: MessageWriter<Footstep>,
    mut gear: MessageWriter<WeaponSound>,
    mut play: MessageWriter<crate::AliasCommand>,
    mut land: MessageWriter<LandSound>,
) {
    let Ok(identity) = identities.get(sound.entity) else {
        diag::warn!(
            Audio,
            "audio: movement sound has no CEntity identity (typed gap)"
        );
        return;
    };
    let player_view = identity.client() == Some(local.0);
    let quieter = identity
        .client()
        .and_then(|id| presented.player(id))
        .is_some_and(|ps| (ps.perks[0] & playerstate_iw4::PERK_QUIETER) != 0);
    let origin_inches = (!player_view).then_some(sound.event.payload.origin);
    if let Some(index) = sound.event.event.landing_surface_index() {
        let surface_flags = (index as u32) << 20;
        let (alias, fallback) = land_aliases(surface_flags, player_view, quieter);
        land.write(LandSound {
            alias,
            fallback,
            origin_inches,
            snd_ent: Some(u32::from(identity.number())),
        });
        return;
    }
    match sound.event.event {
        EntityEventKind::FOOTSTEP_SPRINT
        | EntityEventKind::FOOTSTEP_RUN
        | EntityEventKind::FOOTSTEP_WALK
        | EntityEventKind::FOOTSTEP_PRONE
        | EntityEventKind::JUMP => {}
        EntityEventKind::MANTLE => {
            let alias = mantle_gear_alias(player_view).to_owned();
            let fallback = player_view.then(|| mantle_gear_alias(false).to_owned());
            play.write(crate::AliasCommand::Play(PlayAlias {
                namespace: assets::AssetNamespace::Iw4,
                alias,
                fallback,
                origin_inches,
                snd_ent: Some(u32::from(identity.number())),
            }));
            return;
        }
        _ => unreachable!("EntityMovementSound must carry a movement event"),
    }
    let gait = match sound.event.event {
        EntityEventKind::FOOTSTEP_SPRINT => StepGait::Sprint,

        EntityEventKind::FOOTSTEP_RUN | EntityEventKind::JUMP => StepGait::Run,
        EntityEventKind::FOOTSTEP_WALK => StepGait::Walk,
        EntityEventKind::FOOTSTEP_PRONE => StepGait::Prone,
        _ => unreachable!("footstep arm is only the four EV_FOOTSTEP_* values plus EV_JUMP"),
    };
    let surface_flags = u32::from(sound.event.payload.surf_type) << 20;
    let (alias, fallback) = footstep_aliases(gait, surface_flags, player_view, quieter);
    footsteps.write(Footstep {
        alias,
        fallback,
        origin_inches,
        snd_ent: Some(u32::from(identity.number())),
    });
    gear.write(WeaponSound {
        namespace: assets::AssetNamespace::Iw4,
        alias: gear_rattle_alias(gait, player_view).to_owned(),
        origin_inches,
        snd_ent: Some(u32::from(identity.number())),
    });
}

pub(crate) fn register_entity_event_audio(app: &mut App) {
    app.add_observer(cg_entity_event_sound)
        .add_observer(cg_movement_sound)
        .add_observer(cg_grenade_contact);
}

#[derive(Clone, Debug)]
enum NotetrackSound {
    Bound(usize),
    Unbound(String),
}

#[derive(Clone, Debug)]
struct BoundNotetrack {
    sound: Option<NotetrackSound>,
    rumble: Option<Result<Arc<crate::rumble::Rumble>, String>>,
}

#[derive(Resource, Default)]
pub(crate) struct NotetrackSoundTable {
    owner: Option<(Arc<assets::SoundCatalog>, Arc<assets::WeaponRegistry>)>,
    actions: HashMap<(u32, String), BoundNotetrack>,
    reported: HashSet<(u32, String)>,
}

impl NotetrackSoundTable {
    fn owns(
        &self,
        bank: &Arc<assets::SoundCatalog>,
        weapons: &Arc<assets::WeaponRegistry>,
    ) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|(b, w)| Arc::ptr_eq(b, bank) && Arc::ptr_eq(w, weapons))
    }

    fn bind(bank: &Arc<assets::SoundCatalog>, weapons: &Arc<assets::WeaponRegistry>) -> Self {
        let mut actions = HashMap::new();
        let mut unbound = 0usize;
        let mut rumbles = HashMap::new();
        for weapon in 1..=weapons.len() as u32 {
            let namespace = weapons
                .namespace_of(weapon)
                .unwrap_or(assets::AssetNamespace::Iw4);
            for (note, action) in weapons.notetrack_actions_of(weapon) {
                let sound = action.sound_alias.as_deref().map(|alias| {
                    match bank.index_in(namespace, alias) {
                        Some(index) => NotetrackSound::Bound(index),
                        None => {
                            unbound += 1;
                            NotetrackSound::Unbound(alias.to_owned())
                        }
                    }
                });
                actions.insert(
                    (weapon, note.to_owned()),
                    BoundNotetrack {
                        sound,
                        rumble: action.rumble_alias.as_ref().map(|alias| {
                            rumbles
                                .entry((namespace, alias.clone()))
                                .or_insert_with(|| {
                                    crate::rumble::Rumble::prepare(bank, namespace, alias)
                                })
                                .clone()
                        }),
                    },
                );
            }
        }
        for ((namespace, alias), rumble) in &rumbles {
            if let Err(error) = rumble {
                diag::warn!(
                    Audio,
                    "audio: {namespace:?} rumble `{alias}` refused during binding: {error}"
                );
            }
        }
        diag::info!(
            Audio,
            "audio: viewmodel notetracks bound to the sound bank: actions={} unbound_sounds={unbound} rumbles={} refused_rumbles={}",
            actions.len(),
            rumbles.len(),
            rumbles.values().filter(|value| value.is_err()).count()
        );
        Self {
            owner: Some((Arc::clone(bank), Arc::clone(weapons))),
            actions,
            reported: HashSet::new(),
        }
    }

    fn report_once(&mut self, weapon: u32, note: &str, message: impl FnOnce() -> String) {
        if self.reported.insert((weapon, note.to_owned())) {
            diag::warn!(Audio, "{}", message());
        }
    }
}

pub(crate) fn bind_notetrack_sounds(
    bank: Option<Res<SoundBank>>,
    weapons: Option<Res<PreparedWeapons>>,
    mut table: ResMut<NotetrackSoundTable>,
) {
    let (Some(bank), Some(weapons)) = (bank, weapons) else {
        if table.owner.is_some() {
            *table = NotetrackSoundTable::default();
        }
        return;
    };
    if table.owns(&bank.0, &weapons.0) {
        return;
    }
    *table = NotetrackSoundTable::bind(&bank.0, &weapons.0);
}

pub(crate) fn play_viewmodel_notetrack_messages(
    mut notes: MessageReader<crate::ViewmodelNotetracks>,
    generation: Res<frame::WorldGeneration>,
    presented: Res<net::PresentedSnapshot>,
    local: Res<net::LocalPresentClient>,
    weapons: Option<Res<PreparedWeapons>>,
    bank: Option<Res<SoundBank>>,
    mut table: ResMut<NotetrackSoundTable>,
    mut output: MessageWriter<crate::BoundWeaponSound>,
    mut rumbles: MessageWriter<crate::rumble::PlayRumble>,
) {
    let bound_bank = bank
        .as_deref()
        .zip(weapons.as_deref())
        .filter(|(bank, weapons)| table.owns(&bank.0, &weapons.0))
        .map(|(bank, _)| Arc::clone(&bank.0));
    for batch in notes.read() {
        if batch.generation != *generation
            || batch.client != local.0
            || !presented
                .snapshot()
                .and_then(|snapshot| snapshot.meta.for_client(local.0))
                .is_some_and(|meta| {
                    meta.life_sequence == batch.life
                        && meta.lifecycle == sim::ClientLifecycle::Alive
                })
        {
            continue;
        }
        for name in &batch.names {
            apply_viewmodel_notetrack(
                batch.weapon,
                name,
                bound_bank.as_deref(),
                &mut table,
                &mut output,
                &mut rumbles,
            );
        }
    }
}

fn apply_viewmodel_notetrack(
    weapon: u32,
    note: &str,
    bank: Option<&assets::SoundCatalog>,
    table: &mut NotetrackSoundTable,
    output: &mut MessageWriter<crate::BoundWeaponSound>,
    rumbles: &mut MessageWriter<crate::rumble::PlayRumble>,
) {
    if note.eq_ignore_ascii_case("end") {
        return;
    }
    if note.eq_ignore_ascii_case("NVG_on_powerup") || note.eq_ignore_ascii_case("NVG_off_powerdown")
    {
        table.report_once(weapon, note, || {
            format!("audio: NVG notetrack `{note}` has no cgMedia alias (typed gap)")
        });
    }
    let Some(bank) = bank else {
        table.report_once(weapon, note, || {
            format!("audio: notetrack `{note}` dropped — notetracks are not bound to the installed sound bank")
        });
        return;
    };
    let key = (weapon, note.to_ascii_lowercase());
    let Some(action) = table.actions.get(&key).cloned() else {
        table.report_once(weapon, note, || {
            format!("audio: notetrack `{note}` has no sound or rumble mapping")
        });
        return;
    };
    if let Some(rumble) = &action.rumble {
        match rumble {
            Ok(rumble) => {
                rumbles.write(crate::rumble::PlayRumble {
                    bank_revision: bank.revision(),
                    rumble: Arc::clone(rumble),
                });
            }
            Err(error) => table.report_once(weapon, note, || {
                format!("audio: notetrack `{note}` rumble refused: {error}")
            }),
        }
    }
    match action.sound {
        Some(NotetrackSound::Bound(index)) => {
            output.write(crate::BoundWeaponSound {
                bank_revision: bank.revision(),
                index,
                origin_inches: None,
                snd_ent: Some(crate::SND_ENT_LOCAL),
            });
        }
        Some(NotetrackSound::Unbound(alias)) => {
            table.report_once(weapon, note, || {
                format!(
                    "audio: notetrack `{note}` sound `{alias}` is not in the sound bank (refused when bound)"
                )
            });
        }
        None if action.rumble.is_none() => {
            table.report_once(weapon, note, || {
                format!("audio: notetrack `{note}` has no sound or rumble mapping")
            });
        }
        None => {}
    }
}

fn cg_grenade_contact(
    contact: On<net::EntityGrenadeContact>,
    weapons: Option<Res<PreparedWeapons>>,
    bank: Option<Res<SoundBank>>,
    mut output: MessageWriter<WeaponSound>,
) {
    let payload = &contact.event.payload;
    let surf = usize::try_from(payload.event_parm).unwrap_or(usize::from(payload.surf_type));
    let Some((namespace, alias)) = weapons.as_deref().and_then(|weapons| {
        let bank = bank.as_deref()?;
        let alias = weapons
            .0
            .bounce_sound_alias(payload.weapon, surf, &bank.0)?;
        let namespace = weapons
            .0
            .namespace_of(payload.weapon)
            .unwrap_or(assets::AssetNamespace::Iw4);
        Some((namespace, alias))
    }) else {
        diag::warn!(
            Audio,
            "audio: grenade bounce alias for eventParm {} is unavailable (typed gap)",
            payload.event_parm
        );
        return;
    };
    output.write(WeaponSound {
        namespace,
        alias: alias.to_owned(),
        origin_inches: Some(payload.origin),
        snd_ent: snd_ent_from_number(payload.number),
    });
}
