use asset_core::FamilyId;
use asset_iw4::vertex_decl as iw4;
use render_material::vertex_layout::{VertexLayout, layout_rows};
pub const VERTEX_LIGHTING_SOURCE: u8 = iw4::STREAM_SOURCE_COUNT as u8;
pub const VERTEX_LIGHTING_STREAM: u8 = 1;
const fn sources()
-> [[Option<iw4::StreamSourceLayout>; iw4::STREAM_SOURCE_COUNT + 1]; iw4::VERTEX_TYPE_COUNT] {
    let mut rows = [[None; iw4::STREAM_SOURCE_COUNT + 1]; iw4::VERTEX_TYPE_COUNT];
    let mut vertex = 0;
    while vertex < iw4::VERTEX_TYPE_COUNT {
        let mut source = 0;
        while source <= iw4::STREAM_SOURCE_COUNT {
            rows[vertex][source] = if source == VERTEX_LIGHTING_SOURCE as usize {
                if vertex == iw4::PACKED_VERTEX_TYPE as usize
                    || vertex == iw4::STATICMODELCACHE_VERTEX_TYPE as usize
                {
                    Some(iw4::StreamSourceLayout {
                        stream: VERTEX_LIGHTING_STREAM,
                        offset: 0,
                        decl_type: iw4::D3dDeclType::UByte4N,
                    })
                } else {
                    iw4::source_layout(vertex as u8, 2)
                }
            } else {
                iw4::source_layout(vertex as u8, source as u8)
            };
            source += 1;
        }
        vertex += 1;
    }
    rows
}
const SOURCES: [[Option<iw4::StreamSourceLayout>; iw4::STREAM_SOURCE_COUNT + 1];
    iw4::VERTEX_TYPE_COUNT] = sources();
const ROWS: [&[Option<iw4::StreamSourceLayout>]; iw4::VERTEX_TYPE_COUNT] = layout_rows(&SOURCES);
pub const LAYOUT: VertexLayout = VertexLayout {
    owner: FamilyId::new("t6"),
    destinations: &iw4::DESTINATION_USAGE_TABLE,
    sources: &ROWS,
    stride_overrides: &[
        (iw4::PACKED_VERTEX_TYPE, VERTEX_LIGHTING_STREAM, 4),
        (iw4::STATICMODELCACHE_VERTEX_TYPE, VERTEX_LIGHTING_STREAM, 4),
    ],
};
