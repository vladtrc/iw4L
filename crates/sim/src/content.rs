use crate::match_state::ClassDef;
use crate::spawn::{AuthoredSpawnPoint, MatchBootstrap};
use crate::world::SimBrush;
use weapon_iw4::WeaponCombatFacts;

pub const CONTENT_DIGEST_SCHEME: u64 = 11;

#[derive(Clone, Copy)]
struct Digest(u64);

impl Digest {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    fn byte(&mut self, value: u8) {
        self.0 ^= u64::from(value);
        self.0 = self.0.wrapping_mul(0x100000001b3);
    }

    fn u32(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }

    fn u16(&mut self, value: u16) {
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }

    fn u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }

    fn i32(&mut self, value: i32) {
        self.u32(value as u32);
    }

    fn bool(&mut self, value: bool) {
        self.byte(u8::from(value));
    }

    fn f32(&mut self, value: f32) {
        self.u32(value.to_bits());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.u64(value.len() as u64);
        for byte in value {
            self.byte(*byte);
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}

fn hash_combat(h: &mut Digest, combat: &[WeaponCombatFacts]) {
    h.u64(combat.len() as u64);
    for row in combat {
        h.i32(row.fire_time_ms);
        h.i32(row.fire_delay_ms);
        h.i32(row.raise_time_ms);
        h.i32(row.drop_time_ms);
        h.i32(row.quick_drop_time_ms);
        h.i32(row.inventory_type);
        h.i32(row.weap_class);
        h.i32(row.player_anim_type);
        h.byte(match row.select_requires_ammo_at_0x667 {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        });
        h.byte(match row.offhand_hold_is_cancelable_at_0x681 {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        });
        h.i32(row.reload_time_ms);
        h.i32(row.reload_empty_time_ms);
        h.i32(row.clip_size);
        h.i32(row.start_ammo);
        h.i32(row.max_ammo);
        h.i32(row.ammo_index);
        h.i32(row.clip_index);
        h.i32(row.fire_type);
        h.i32(row.impact_type);
        h.i32(row.shots_per_fire);
        h.i32(row.burst_cooldown_ms);
        h.bool(row.bolt_action);
        h.i32(row.rechamber_time_ms);
        h.i32(row.rechamber_bolt_time_ms);
        h.i32(row.rechamber_bolt_delay_ms);
        h.bool(row.segmented_reload);
        h.i32(row.reload_start_time_ms);
        h.i32(row.reload_end_time_ms);
        h.i32(row.reload_ammo_add);
        h.i32(row.reload_add_time_ms);
        h.i32(row.reload_empty_add_time_ms);
        h.i32(row.reload_start_add_time_ms);
        h.i32(row.reload_start_add);
        h.bool(row.no_partial_reload);
        h.bool(row.inherits_perks);
        h.i32(row.sprint_raise_time_ms);
        h.i32(row.sprint_drop_time_ms);
        h.i32(row.damage);
        h.i32(row.min_damage);
        h.f32(row.max_damage_range);
        h.f32(row.min_damage_range);
        h.f32(row.hip_spread_stand_min);
        h.f32(row.hip_spread_ducked_min);
        h.f32(row.hip_spread_prone_min);
        h.f32(row.hip_spread_stand_max);
        h.f32(row.hip_spread_ducked_max);
        h.f32(row.hip_spread_prone_max);
        h.f32(row.hip_spread_decay_rate);
        h.f32(row.hip_spread_fire_add);
        h.f32(row.hip_spread_turn_add);
        h.f32(row.hip_spread_move_add);
        h.f32(row.hip_spread_ducked_decay);
        h.f32(row.hip_spread_prone_decay);
        h.f32(row.ads_spread);
        h.bool(row.aim_down_sight);
        h.bool(row.no_ads_when_mag_empty);
        h.f32(row.ads_in_rate);
        h.f32(row.ads_out_rate);
        h.bool(row.rechamber_while_ads);
        h.bool(row.ads_fire_only);
        h.i32(row.melee_damage);
        h.i32(row.overlay_reticle);
        h.i32(row.melee_time_ms);
        h.i32(row.melee_delay_ms);
        h.i32(row.melee_charge_time_ms);
        h.i32(row.melee_charge_delay_ms);
        h.u32(row.knife_model);
        h.i32(row.quick_raise_time_ms);
    }
}

fn hash_classes(h: &mut Digest, classes: &[ClassDef]) {
    h.u64(classes.len() as u64);
    for c in classes {
        h.u32(c.id.0);
        h.u32(c.revision);
        h.u32(c.primary);
        h.u32(c.secondary);
        for id in c.primary_attachments {
            h.u32(id);
        }
        for id in c.secondary_attachments {
            h.u32(id);
        }
        h.u32(c.lethal);
        h.u32(c.tactical);
        for id in c.perks {
            h.u32(id);
        }
        h.bytes(c.deathstreak.as_bytes());
        h.bool(c.locked);
    }
}

fn hash_spawns(h: &mut Digest, spawns: &[AuthoredSpawnPoint]) {
    h.u64(spawns.len() as u64);
    for spawn in spawns {
        h.bytes(spawn.classname.as_bytes());
        for value in spawn.origin {
            h.f32(value);
        }
        for value in spawn.angles {
            h.f32(value);
        }
    }
}

fn hash_collision(h: &mut Digest, brushes: &[SimBrush]) {
    h.u64(brushes.len() as u64);
    for brush in brushes {
        h.u64(brush.planes.len() as u64);
        for plane in &brush.planes {
            for value in plane {
                h.f32(*value);
            }
        }
        h.u32(brush.contents);
        h.u64(brush.plane_surface_flags.len() as u64);
        for f in &brush.plane_surface_flags {
            h.u32(*f);
        }
        h.u16(brush.glass_encoded);
    }
}

fn hash_equipment(h: &mut Digest, rows: &[crate::EquipmentRuntimeFacts]) {
    h.u64(rows.len() as u64);
    for row in rows {
        h.i32(row.offhand_class);
        h.i32(row.start_ammo);
        h.i32(row.clip_size);
        h.i32(row.impact_damage);
        h.i32(row.fuse_time_ms);
        h.i32(row.hold_fire_time_ms);
        h.bool(row.cook_off_hold);
        h.bool(row.proj_impact_explode);
        h.bool(row.stick_to_players);
        h.i32(row.explosion_radius);
        h.i32(row.explosion_radius_min);
        h.i32(row.explosion_inner_damage);
        h.i32(row.explosion_outer_damage);
        h.i32(row.projectile_speed);
        h.i32(row.projectile_speed_up);
        h.i32(row.projectile_activate_dist);
        h.i32(row.projectile_explosion_type);
        for coefficients in [row.parallel_bounce, row.perpendicular_bounce] {
            h.bool(coefficients.is_some());
            if let Some(values) = coefficients {
                for value in values {
                    h.f32(value);
                }
            }
        }
    }
}

pub fn content_digest_v1(
    combat: &[WeaponCombatFacts],
    bootstrap: &MatchBootstrap,
    clip_brushes: &[SimBrush],
) -> u64 {
    let mut h = Digest::new();
    h.u64(CONTENT_DIGEST_SCHEME);
    hash_combat(&mut h, combat);
    hash_classes(&mut h, &bootstrap.classes);
    hash_spawns(&mut h, &bootstrap.spawns);
    hash_collision(&mut h, clip_brushes);
    h.u32(bootstrap.respawn_delay_ticks);
    h.finish()
}

fn hash_script_models(h: &mut Digest, script_models: &[crate::EntityCollisionCapabilities]) {
    h.u32(script_models.len() as u32);
    for capabilities in script_models {
        match capabilities.owner {
            crate::AuthorityModelOwner::ScriptModel(id) => {
                h.u32(1);
                h.u32(id.0);
            }
        }
        h.u32(capabilities.epoch().digest_tag());
        if let Some(dobj) = &capabilities.dobj {
            h.u32(1);
            h.bytes(dobj.current_model.as_bytes());
            h.u32(dobj.model_revision);
            h.u32(dobj.pose_revision);
            let bones = dobj
                .current_collision
                .as_ref()
                .map_or(&[][..], |geometry| geometry.bones.as_slice());
            h.u32(bones.len() as u32);
            for bone in bones {
                h.u32(u32::from(bone.bone));
                h.u32(u32::from(bone.part_classification));
                for value in bone
                    .center
                    .iter()
                    .chain(bone.axes.iter().flatten())
                    .chain(bone.half_size.iter())
                {
                    h.f32(*value);
                }
            }
        } else {
            h.u32(0);
        }
        h.u32(capabilities.linked_brushes.len() as u32);
        for brush in &capabilities.linked_brushes {
            h.u32(brush.cmodel_handle);
            for value in brush.origin.iter().chain(brush.angles.iter()) {
                h.f32(*value);
            }
        }
    }
}

pub fn content_digest_v2(
    combat: &[WeaponCombatFacts],
    equipment: &[crate::EquipmentRuntimeFacts],
    bootstrap: &MatchBootstrap,
    clip_brushes: &[SimBrush],
    script_models: &[crate::EntityCollisionCapabilities],
) -> u64 {
    let mut h = Digest::new();
    h.u64(CONTENT_DIGEST_SCHEME);
    hash_combat(&mut h, combat);
    hash_equipment(&mut h, equipment);
    hash_classes(&mut h, &bootstrap.classes);
    hash_spawns(&mut h, &bootstrap.spawns);
    hash_collision(&mut h, clip_brushes);
    hash_script_models(&mut h, script_models);
    h.u32(bootstrap.respawn_delay_ticks);
    h.finish()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContentComponents {
    pub map: u64,

    pub models: u64,

    pub weapons: u64,

    pub classes: u64,
}

pub fn content_components_v2(
    combat: &[WeaponCombatFacts],
    equipment: &[crate::EquipmentRuntimeFacts],
    bootstrap: &MatchBootstrap,
    clip_brushes: &[SimBrush],
    script_models: &[crate::EntityCollisionCapabilities],
) -> ContentComponents {
    let component = |tag: u8| {
        let mut h = Digest::new();
        h.u64(CONTENT_DIGEST_SCHEME);
        h.byte(tag);
        h
    };

    let mut map = component(b'M');
    hash_spawns(&mut map, &bootstrap.spawns);
    hash_collision(&mut map, clip_brushes);

    let mut models = component(b'D');
    hash_script_models(&mut models, script_models);

    let mut weapons = component(b'W');
    hash_combat(&mut weapons, combat);
    hash_equipment(&mut weapons, equipment);

    let mut classes = component(b'C');
    hash_classes(&mut classes, &bootstrap.classes);
    classes.u32(bootstrap.respawn_delay_ticks);

    ContentComponents {
        map: map.finish(),
        models: models.finish(),
        weapons: weapons.finish(),
        classes: classes.finish(),
    }
}

pub fn content_digest_v0(combat: &[WeaponCombatFacts], classes: &[ClassDef]) -> u64 {
    let bootstrap = MatchBootstrap {
        classes: classes.to_vec(),
        ..MatchBootstrap::default()
    };
    content_digest_v1(combat, &bootstrap, &[])
}
