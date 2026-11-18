use asset_core::FamilyId;
use asset_iw4::vertex_decl as iw4;
use ext_t5_fastfile::vertex_decl as native;
use render_material::vertex_layout::{VertexLayout, layout_rows};
const VERTEX_COUNT: usize = native::SOURCE_LAYOUT_TABLE.len();
pub use render_material::WORLD_LAYER_HOST_STRIDE;
const fn sources() -> [[Option<iw4::StreamSourceLayout>; 10]; VERTEX_COUNT] {
    let mut rows = [[None; 10]; VERTEX_COUNT];
    let mut vertex = 0;
    while vertex < VERTEX_COUNT {
        let mut source = 0;
        while source < 10 {
            let [stream, offset, raw] = native::SOURCE_LAYOUT_TABLE[vertex][source];
            if stream != 255 {
                rows[vertex][source] = Some(iw4::StreamSourceLayout {
                    stream,
                    offset,
                    decl_type: iw4::D3dDeclType::from_raw(raw),
                });
            }
            source += 1;
        }
        vertex += 1;
    }
    rows
}
const SOURCES: [[Option<iw4::StreamSourceLayout>; 10]; VERTEX_COUNT] = sources();
const ROWS: [&[Option<iw4::StreamSourceLayout>]; VERTEX_COUNT] = layout_rows(&SOURCES);
pub const LAYOUT: VertexLayout = VertexLayout {
    owner: FamilyId::new("t5"),
    destinations: &native::DESTINATION_USAGE_TABLE,
    sources: &ROWS,
    stride_overrides: &[],
};
