#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotetrackConvention {
    #[default]
    SoundMap,

    InlinePrefix,
}

impl NotetrackConvention {
    pub const fn dump_token(self) -> &'static str {
        match self {
            Self::SoundMap => "sound_map",
            Self::InlinePrefix => "inline_prefix",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponSoundAliases {
    pub fire: Option<String>,
    pub fire_player: Option<String>,
    pub empty_fire: Option<String>,
    pub empty_fire_player: Option<String>,
    pub melee_swipe: Option<String>,
    pub melee_swipe_player: Option<String>,
    pub melee_hit: Option<String>,
    pub melee_miss: Option<String>,
    pub pickup: Option<String>,
    pub pickup_player: Option<String>,
    pub ammo_pickup: Option<String>,
    pub ammo_pickup_player: Option<String>,
    pub detonate: Option<String>,
    pub detonate_player: Option<String>,
    pub pullback: Option<String>,
    pub pullback_player: Option<String>,
    pub reload: Option<String>,
    pub reload_player: Option<String>,
    pub reload_empty: Option<String>,
    pub reload_empty_player: Option<String>,
    pub reload_start: Option<String>,
    pub reload_start_player: Option<String>,
    pub reload_end: Option<String>,
    pub reload_end_player: Option<String>,
    pub rechamber: Option<String>,
    pub rechamber_player: Option<String>,
    pub alt_switch: Option<String>,
    pub alt_switch_player: Option<String>,
    pub raise: Option<String>,
    pub raise_player: Option<String>,
    pub first_raise: Option<String>,
    pub first_raise_player: Option<String>,
    pub putaway: Option<String>,
    pub putaway_player: Option<String>,
    pub proj_explosion: Option<String>,

    pub projectile: Option<String>,

    pub proj_ignition_sound: Option<String>,

    pub bounce: [Option<String>; asset_iw4::size::SURF_TYPE_NUM],

    pub notetrack_sound_map: Vec<(String, String)>,

    pub notetrack_rumble_map: Vec<(String, String)>,

    pub fire_player_akimbo: Option<String>,

    pub fire_start: Option<String>,
    pub fire_start_player: Option<String>,

    pub fire_loop: Option<String>,
    pub fire_loop_player: Option<String>,

    pub fire_stop: Option<String>,
    pub fire_stop_player: Option<String>,

    pub fire_last: Option<String>,

    pub fire_last_player: Option<String>,

    /// None means no authored dependency table; Some(empty) preserves an authored empty table.
    pub dependency_aliases: Option<Vec<String>>,

    pub fire_ptr_kind: Option<&'static str>,
    pub fire_player_ptr_kind: Option<&'static str>,
    pub reload_player_ptr_kind: Option<&'static str>,

    pub notetrack_convention: NotetrackConvention,
}

macro_rules! weapon_sound_slots {
    ($($variant:ident => $field:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(usize)]
        pub enum WeaponSoundSlot {
            $($variant),+
        }

        impl WeaponSoundSlot {
            pub const ALL: [Self; weapon_sound_slots!(@count $($variant),+)] = [
                $(Self::$variant),+
            ];
        }

        impl WeaponSoundAliases {
            pub(crate) fn hint(&self, slot: WeaponSoundSlot) -> Option<&str> {
                match slot {
                    $(WeaponSoundSlot::$variant => self.$field.as_deref()),+
                }
            }
        }
    };
    (@count $($variant:ident),+) => {
        <[()]>::len(&[$(weapon_sound_slots!(@unit $variant)),+])
    };
    (@unit $variant:ident) => { () };
}

weapon_sound_slots! {
    Fire => fire,
    FirePlayer => fire_player,
    EmptyFire => empty_fire,
    EmptyFirePlayer => empty_fire_player,
    MeleeSwipe => melee_swipe,
    MeleeSwipePlayer => melee_swipe_player,
    MeleeHit => melee_hit,
    MeleeMiss => melee_miss,
    Pickup => pickup,
    PickupPlayer => pickup_player,
    AmmoPickup => ammo_pickup,
    AmmoPickupPlayer => ammo_pickup_player,
    Detonate => detonate,
    DetonatePlayer => detonate_player,
    Pullback => pullback,
    PullbackPlayer => pullback_player,
    Reload => reload,
    ReloadPlayer => reload_player,
    ReloadEmpty => reload_empty,
    ReloadEmptyPlayer => reload_empty_player,
    ReloadStart => reload_start,
    ReloadStartPlayer => reload_start_player,
    ReloadEnd => reload_end,
    ReloadEndPlayer => reload_end_player,
    Rechamber => rechamber,
    RechamberPlayer => rechamber_player,
    AltSwitch => alt_switch,
    AltSwitchPlayer => alt_switch_player,
    Raise => raise,
    RaisePlayer => raise_player,
    FirstRaise => first_raise,
    FirstRaisePlayer => first_raise_player,
    Putaway => putaway,
    PutawayPlayer => putaway_player,
    ProjectileExplosion => proj_explosion,
    Projectile => projectile,
    ProjIgnition => proj_ignition_sound,
    FireLast => fire_last,
    FireLastPlayer => fire_last_player,
    FireStart => fire_start,
    FireStartPlayer => fire_start_player,
    FireLoop => fire_loop,
    FireLoopPlayer => fire_loop_player,
    FireStop => fire_stop,
    FireStopPlayer => fire_stop_player,
}

impl WeaponSoundAliases {
    pub fn reachable_aliases(&self) -> Vec<&str> {
        let mut out = Vec::new();
        let slots = [
            self.fire.as_deref(),
            self.fire_player.as_deref(),
            self.empty_fire.as_deref(),
            self.empty_fire_player.as_deref(),
            self.melee_swipe.as_deref(),
            self.melee_swipe_player.as_deref(),
            self.melee_hit.as_deref(),
            self.melee_miss.as_deref(),
            self.pickup.as_deref(),
            self.pickup_player.as_deref(),
            self.ammo_pickup.as_deref(),
            self.ammo_pickup_player.as_deref(),
            self.detonate.as_deref(),
            self.detonate_player.as_deref(),
            self.pullback.as_deref(),
            self.pullback_player.as_deref(),
            self.reload.as_deref(),
            self.reload_player.as_deref(),
            self.reload_empty.as_deref(),
            self.reload_empty_player.as_deref(),
            self.reload_start.as_deref(),
            self.reload_start_player.as_deref(),
            self.reload_end.as_deref(),
            self.reload_end_player.as_deref(),
            self.rechamber.as_deref(),
            self.rechamber_player.as_deref(),
            self.alt_switch.as_deref(),
            self.alt_switch_player.as_deref(),
            self.raise.as_deref(),
            self.raise_player.as_deref(),
            self.first_raise.as_deref(),
            self.first_raise_player.as_deref(),
            self.putaway.as_deref(),
            self.putaway_player.as_deref(),
            self.proj_explosion.as_deref(),
            self.projectile.as_deref(),
            self.proj_ignition_sound.as_deref(),
            self.fire_player_akimbo.as_deref(),
            self.fire_start.as_deref(),
            self.fire_start_player.as_deref(),
            self.fire_loop.as_deref(),
            self.fire_loop_player.as_deref(),
            self.fire_stop.as_deref(),
            self.fire_stop_player.as_deref(),
            self.fire_last.as_deref(),
            self.fire_last_player.as_deref(),
        ];
        for slot in slots.into_iter().flatten() {
            if !slot.is_empty() {
                out.push(slot);
            }
        }
        for name in &self.bounce {
            if let Some(s) = name.as_deref().filter(|s| !s.is_empty()) {
                out.push(s);
            }
        }
        for (_, alias) in &self.notetrack_sound_map {
            if !alias.is_empty() {
                out.push(alias.as_str());
            }
        }
        out.extend(
            self.dependency_aliases
                .iter()
                .flatten()
                .map(String::as_str)
                .filter(|s| !s.is_empty()),
        );
        out
    }
}

pub fn parse_prefixed_notetrack<'a>(note: &'a str, prefix: &str) -> Option<&'a str> {
    let (head, tail) = note.split_at_checked(prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then_some(tail)
        .filter(|alias| !alias.is_empty())
}
