pub const MAX_KNOTS: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AiAccuracyGraph {
    knots: [[f32; 2]; MAX_KNOTS],
    len: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AiAccuracyGraphs {
    pub vs_ai: AiAccuracyGraph,
    pub vs_player: AiAccuracyGraph,
}

pub const GRAPH_RANGE: f32 = 4000.0;

impl AiAccuracyGraph {
    pub fn new(knots: impl IntoIterator<Item = [f32; 2]>) -> Self {
        let mut graph = Self::default();
        for knot in knots.into_iter().take(MAX_KNOTS) {
            graph.knots[usize::from(graph.len)] = knot;
            graph.len += 1;
        }
        graph
    }

    pub fn is_empty(&self) -> bool {
        self.len < 2
    }

    pub fn accuracy(&self, distance: f32) -> f32 {
        let knots = &self.knots[..usize::from(self.len)];
        let x = (distance / GRAPH_RANGE).min(1.0);
        let Some(i) = (1..knots.len()).find(|&i| x <= knots[i][0]) else {
            return -1.0;
        };
        let ([x0, y0], [x1, y1]) = (knots[i - 1], knots[i]);
        y0 + (y1 - y0) * ((x - x0) / (x1 - x0))
    }
}
