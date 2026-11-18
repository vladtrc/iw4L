use asset_game::{FlameStreamView, FlameTube, WeaponFlameStream};

pub fn capture(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    geometry: &ext_t5_fastfile::WeaponGeometry,
) -> Option<Box<WeaponFlameStream>> {
    use ext_t5_fastfile::size as sz;
    let body = geometry.weap_def?;
    let mut views: [FlameStreamView; 2] = Default::default();
    for (view, (off, names)) in views.iter_mut().zip(
        [
            sz::WEAPON_DEF_FLAME_TABLE_FIRST_PERSON_OFF,
            sz::WEAPON_DEF_FLAME_TABLE_THIRD_PERSON_OFF,
        ]
        .into_iter()
        .zip(&geometry.flame_material_names),
    ) {
        let table = match stream.ptr_at(body, off).ok()? {
            ext_t5_fastfile::ZonePtr::Offset(q) => stream.resolve_alias(q),
            _ => return None,
        };
        let f = |i: usize| stream.f32_at(table, i * 4).unwrap_or(0.0);
        let tube = |i: usize| FlameTube {
            size: [f(i), f(i + 1)],
            length: f(i + 2),
            segments: f(i + 3),
            anim_loop_s: f(i + 4),
        };
        let name = |slot: usize| {
            names[slot]
                .and_then(|ptr| stream.cstr(ptr).ok())
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
        };
        *view = FlameStreamView {
            chunk_gravity: [f(0), f(1)],
            chunk_max_size: f(2),
            chunk_size: [f(3), f(4)],
            chunk_size_rand: [f(5), f(6)],
            chunk_dist_scalar: f(7),
            chunk_speed: f(10),
            chunk_decel: f(11),
            chunk_life_s: f(13),
            chunk_rot_rand: f(28),
            fire_interval_s: [f(18), f(19)],
            fire_first_life_frac: f(20),
            fire_min_life_frac: f(22),
            fire_min_life_age_s: [f(23), f(24)],
            fuel: tube(40),
            flame: tube(45),
            fire_life_s: f(58),
            fire_life_rand_s: f(59),
            fire_speed_scale: f(60),
            fire_speed_scale_rand: f(61),
            fire_vel_add_z: f(62),
            fire_vel_add_z_rand: f(63),
            fire_vel_add_sideways: f(64),
            fire_gravity: [f(65), f(66)],
            fire_max_rot_vel: f(67),
            fire_friction: f(68),
            fire_end_size_add: f(69),
            fire_start_size_scale: f(70),
            fire_end_size_scale: f(71),
            materials: std::array::from_fn(name),
        };
    }
    Some(Box::new(WeaponFlameStream { views }))
}
