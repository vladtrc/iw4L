use asset_core::FamilyId;
use asset_iw4::vertex_decl as iw4;

/// A game supplies the complete routing and host stream layout it publishes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct VertexLayout {
    pub owner: FamilyId,
    pub destinations: &'static [[u8; 2]],
    pub sources: &'static [&'static [Option<iw4::StreamSourceLayout>]],
    pub stride_overrides: &'static [(u8, u8, u16)],
}
impl VertexLayout {
    pub fn source_count(self) -> usize {
        self.sources.iter().map(|row| row.len()).max().unwrap_or(0)
    }
    pub fn destination_usage(self, dest: u8) -> Option<(u8, u8)> {
        self.destinations
            .get(usize::from(dest))
            .map(|&[usage, index]| (usage, index))
    }
    pub fn source_layout(self, vertex_type: u8, source: u8) -> Option<iw4::StreamSourceLayout> {
        self.sources
            .get(usize::from(vertex_type))?
            .get(usize::from(source))
            .copied()
            .flatten()
    }
    pub fn host_stream_stride(self, vertex_type: u8, stream: u8) -> Option<u16> {
        if let Some(&(_, _, stride)) = self
            .stride_overrides
            .iter()
            .find(|&&(vertex, slot, _)| vertex == vertex_type && slot == stream)
        {
            return Some(stride);
        }
        self.sources
            .get(usize::from(vertex_type))?
            .iter()
            .flatten()
            .filter(|layout| layout.stream == stream)
            .filter_map(|layout| {
                layout
                    .decl_type
                    .byte_len()
                    .map(|len| u16::from(layout.offset) + u16::from(len))
            })
            .max()
    }
}

pub const fn layout_rows<const R: usize, const C: usize>(
    table: &'static [[Option<iw4::StreamSourceLayout>; C]; R],
) -> [&'static [Option<iw4::StreamSourceLayout>]; R] {
    let mut rows: [&'static [Option<iw4::StreamSourceLayout>]; R] = [&[]; R];
    let mut index = 0;
    while index < R {
        rows[index] = &table[index];
        index += 1;
    }
    rows
}
const fn iw4_sources()
-> [[Option<iw4::StreamSourceLayout>; iw4::STREAM_SOURCE_COUNT]; iw4::VERTEX_TYPE_COUNT] {
    let mut rows = [[None; iw4::STREAM_SOURCE_COUNT]; iw4::VERTEX_TYPE_COUNT];
    let mut vertex = 0;
    while vertex < iw4::VERTEX_TYPE_COUNT {
        let mut source = 0;
        while source < iw4::STREAM_SOURCE_COUNT {
            rows[vertex][source] = iw4::source_layout(vertex as u8, source as u8);
            source += 1;
        }
        vertex += 1;
    }
    rows
}
const IW4_SOURCES: [[Option<iw4::StreamSourceLayout>; iw4::STREAM_SOURCE_COUNT];
    iw4::VERTEX_TYPE_COUNT] = iw4_sources();
const IW4_ROWS: [&[Option<iw4::StreamSourceLayout>]; iw4::VERTEX_TYPE_COUNT] =
    layout_rows(&IW4_SOURCES);
pub const IW4_VERTEX_LAYOUT: VertexLayout = VertexLayout {
    owner: FamilyId::IW4,
    destinations: &iw4::DESTINATION_USAGE_TABLE,
    sources: &IW4_ROWS,
    stride_overrides: &[],
};

impl std::fmt::Debug for VertexLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VertexLayout")
            .field("owner", &self.owner)
            .field("vertex_types", &self.sources.len())
            .field("destinations", &self.destinations.len())
            .finish_non_exhaustive()
    }
}
