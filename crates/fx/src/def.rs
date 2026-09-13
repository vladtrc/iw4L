#[derive(Clone, Copy, Debug)]
pub struct FxElemDefInfo {
    pub elem_type: u8,

    pub spawn_a: i32,
    pub spawn_b: i32,
    pub delay_base: i32,
    pub delay_amp: i32,
    pub life_base: i32,
    pub life_amp: i32,
    pub flags: i32,
    pub visual_count: u8,
    pub vis_state_interval_count: u8,

    pub spawn_range_base: f32,
    pub spawn_range_amp: f32,

    pub spawn_origin: [[f32; 2]; 3],
    pub spawn_offset_radius_base: f32,
    pub spawn_offset_radius_amp: f32,
    pub spawn_offset_height_base: f32,
    pub spawn_offset_height_amp: f32,

    pub has_effect_on_impact: bool,
    pub has_effect_on_death: bool,
    pub has_effect_emitted: bool,

    pub sort_order: u8,

    pub spark_count: i32,
    pub spark_vel_min: f32,
    pub spark_vel_max: f32,
    pub spark_vel_cone_frac: f32,
    pub spark_gravity: f32,
    pub spark_length: f32,
    pub spark_loop_time: f32,
    pub spark_boost_time: f32,
    pub spark_boost_factor: f32,

    pub spark_bounce_frac: f32,
    pub spark_bounce_rand: f32,

    pub inv_split_dist: f32,
    pub inv_split_arc_dist: f32,
    pub inv_split_time: f32,

    pub gravity_base: f32,
    pub gravity_amp: f32,

    pub reflection_base: f32,
    pub reflection_amp: f32,

    pub coll_mins: [f32; 3],
    pub coll_maxs: [f32; 3],

    pub use_item_clip: u8,

    pub spawn_angles: [[f32; 2]; 3],
    pub angular_velocity: [[f32; 2]; 3],
}

impl Default for FxElemDefInfo {
    fn default() -> Self {
        Self {
            elem_type: 0,
            spawn_a: 0,
            spawn_b: 0,
            delay_base: 0,
            delay_amp: 0,
            life_base: 0,
            life_amp: 0,
            flags: 0,
            visual_count: 0,
            vis_state_interval_count: 0,
            spawn_range_base: 0.0,
            spawn_range_amp: 0.0,
            spawn_origin: [[0.0; 2]; 3],
            spawn_offset_radius_base: 0.0,
            spawn_offset_radius_amp: 0.0,
            spawn_offset_height_base: 0.0,
            spawn_offset_height_amp: 0.0,
            has_effect_on_impact: false,
            has_effect_on_death: false,
            has_effect_emitted: false,
            sort_order: 0,
            spark_count: 0,
            spark_vel_min: 0.0,
            spark_vel_max: 0.0,
            spark_vel_cone_frac: 0.0,
            spark_gravity: 0.0,
            spark_length: 0.0,
            spark_loop_time: 0.0,
            spark_boost_time: 0.0,
            spark_boost_factor: 0.0,
            spark_bounce_frac: 0.0,
            spark_bounce_rand: 0.0,
            inv_split_dist: 0.0,
            inv_split_arc_dist: 0.0,
            inv_split_time: 0.0,
            gravity_base: 0.0,
            gravity_amp: 0.0,
            reflection_base: 0.0,
            reflection_amp: 0.0,
            coll_mins: [0.0; 3],
            coll_maxs: [0.0; 3],
            use_item_clip: 0,
            spawn_angles: [[0.0; 2]; 3],
            angular_velocity: [[0.0; 2]; 3],
        }
    }
}

impl FxElemDefInfo {
    pub fn keep_alive_by_child(self) -> bool {
        self.has_effect_on_impact || self.has_effect_on_death || self.has_effect_emitted
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FxEffectDefInfo<'a> {
    pub msec_looping_life: i32,
    pub looping_count: i32,
    pub one_shot_count: i32,
    pub emission_count: i32,
    pub elems: &'a [FxElemDefInfo],
}

impl FxEffectDefInfo<'_> {
    pub fn total_elem_defs(self) -> i32 {
        self.looping_count
            .saturating_add(self.one_shot_count)
            .saturating_add(self.emission_count)
    }
}
