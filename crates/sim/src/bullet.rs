use weapon_iw4::WeaponCombatFacts;

pub fn bullet_damage_at_distance(facts: &WeaponCombatFacts, dist: f32) -> i32 {
    let base = facts.damage;
    let min = if facts.min_damage > 0 {
        facts.min_damage
    } else {
        base
    };
    let mut damage = base;
    let range = facts.min_damage_range - facts.max_damage_range;
    if base != min && range != 0.0 {
        if facts.max_damage_range <= dist {
            if facts.min_damage_range <= dist {
                damage = min;
            } else {
                let lerp = (dist - facts.max_damage_range) / range;
                let lerp = lerp.clamp(0.0, 1.0);
                damage = ((base as f32) * (1.0 - lerp) + (min as f32) * lerp) as i32;
            }
        }
    }
    damage.max(0)
}

pub fn angles_to_forward(angles: [f32; 3]) -> [f32; 3] {
    let yaw = angles[1].to_radians();
    let pitch = angles[0].to_radians();
    let cy = yaw.cos();
    let sy = yaw.sin();
    let sp = pitch.sin();
    let cp = pitch.cos();
    [cp * cy, cp * sy, -sp]
}
