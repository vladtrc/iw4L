use super::*;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Clone, Copy)]
struct Frontier {
    cost: f32,
    node: u16,
}
impl PartialEq for Frontier {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Frontier {}
impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.node.cmp(&self.node))
    }
}
impl PathNodes {
    pub(crate) fn facing_node(
        &self,
        from: u16,
        observer: u16,
        team: u8,
        mut can_cross: impl FnMut(u16, u16) -> bool,
    ) -> Option<u16> {
        if usize::from(from) >= self.data.len() || usize::from(observer) >= self.data.len() {
            return None;
        }
        let mut costs = vec![f32::INFINITY; self.data.len()];
        let mut frontier = BinaryHeap::new();
        costs[usize::from(from)] = 0.;
        frontier.push(Frontier {
            cost: 0.,
            node: from,
        });
        while let Some(Frontier { cost, node }) = frontier.pop() {
            if cost != costs[usize::from(node)] {
                continue;
            }
            if self.visible(node, observer) {
                return Some(node);
            }
            for link in &self.links[usize::from(node)] {
                let blocked = if team == 0 {
                    link.flags != 0
                } else {
                    link.bad_place_count[usize::from(team.min(3) - 1)] != 0
                };
                if link.disconnect_count != 0 || blocked || !can_cross(node, link.node) {
                    continue;
                }
                let next = usize::from(link.node);
                let proposed = cost + link.distance;
                if proposed < costs[next] {
                    costs[next] = proposed;
                    frontier.push(Frontier {
                        cost: proposed,
                        node: link.node,
                    });
                }
            }
        }
        None
    }
}
