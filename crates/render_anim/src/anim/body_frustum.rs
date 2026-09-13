#[derive(Clone, Copy, Debug)]
pub struct AdmittedCellVis<'a> {
    pub words: &'a [u32],
    pub cell_count: usize,

    pub vis_all: bool,
}

impl AdmittedCellVis<'_> {
    pub fn contains(self, cell: usize) -> bool {
        if self.vis_all {
            return true;
        }
        vis_lsb(self.words, cell)
    }
}

fn vis_lsb(words: &[u32], index: usize) -> bool {
    let word = index / 32;
    let bit = index % 32;
    words.get(word).is_some_and(|w| (*w & (1u32 << bit)) != 0)
}
