use crate::vertex_layout::VertexLayoutFamily;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeVertexDecl {
    pub family: VertexLayoutFamily,
    pub name: String,
    pub stream_count: u8,
    pub has_optional_source: u8,
    pub routing: [[u8; 2]; asset_iw4::vertex_decl::ROUTING_COUNT],
}

impl RuntimeVertexDecl {
    pub fn routed(&self) -> &[[u8; 2]] {
        let count = usize::from(self.stream_count).min(asset_iw4::vertex_decl::ROUTING_COUNT);
        &self.routing[..count]
    }

    pub fn routed_pairs_csv(&self) -> String {
        self.routed()
            .iter()
            .map(|pair| format!("{}:{}", pair[0], pair[1]))
            .collect::<Vec<_>>()
            .join(",")
    }
}
