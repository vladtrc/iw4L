#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XModel {
    pub num_bones: u8,
    pub num_root_bones: u8,
    pub scale: f32,
    pub no_scale_part_bits: [u32; 6],
    pub bone_names: u32,
    pub parent_list: u32,
    pub quats: u32,
    pub trans: u32,
    pub part_classification: u32,
    pub base_mat: u32,
    pub bone_info: u32,
    pub radius: f32,
}
