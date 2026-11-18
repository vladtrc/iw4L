#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlameTube {
    pub size: [f32; 2],
    pub length: f32,
    pub segments: f32,
    pub anim_loop_s: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlameStreamView {
    pub chunk_gravity: [f32; 2],
    pub chunk_max_size: f32,
    pub chunk_size: [f32; 2],
    pub chunk_size_rand: [f32; 2],
    pub chunk_dist_scalar: f32,
    pub chunk_speed: f32,
    pub chunk_decel: f32,
    pub chunk_life_s: f32,
    pub chunk_rot_rand: f32,
    pub fire_interval_s: [f32; 2],
    pub fire_first_life_frac: f32,
    pub fire_min_life_frac: f32,
    pub fire_min_life_age_s: [f32; 2],
    pub fuel: FlameTube,
    pub flame: FlameTube,
    pub fire_life_s: f32,
    pub fire_life_rand_s: f32,
    pub fire_speed_scale: f32,
    pub fire_speed_scale_rand: f32,
    pub fire_vel_add_z: f32,
    pub fire_vel_add_z_rand: f32,
    pub fire_vel_add_sideways: f32,
    pub fire_gravity: [f32; 2],
    pub fire_max_rot_vel: f32,
    pub fire_friction: f32,
    pub fire_end_size_add: f32,
    pub fire_start_size_scale: f32,
    pub fire_end_size_scale: f32,
    /// Normalized renderer material roles: fire, smoke, heat, drips, fuel, fuel2,
    /// flame, flame2.
    pub materials: [Option<String>; 8],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeaponFlameStream {
    pub views: [FlameStreamView; 2],
}

impl WeaponFlameStream {
    pub fn view(&self, player_view: bool) -> &FlameStreamView {
        &self.views[usize::from(!player_view)]
    }
}
