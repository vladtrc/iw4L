use core::fmt::{self, Write as _};

pub trait Gap: Copy + PartialEq + 'static {
    const ALL: &'static [Self];

    fn name(self) -> &'static str;

    fn is_standing(self) -> bool;

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|g| *g == self)
            .expect("Gap::ALL does not list this variant")
    }
}

pub trait GapCause: fmt::Display {
    type Gap: Gap;

    fn gap(&self) -> Self::Gap;
}

enum GapState<C> {
    Absent,

    Standing,

    Raised(C),
}

pub struct GapLedger<C: GapCause, const N: usize> {
    live: [GapState<C>; N],

    hits: [u64; N],
}

impl<C: GapCause, const N: usize> Default for GapLedger<C, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: GapCause, const N: usize> GapLedger<C, N> {
    pub fn new() -> Self {
        const {
            assert!(
                N == C::Gap::ALL.len(),
                "GapLedger width must be Gap::ALL.len()"
            );
        }
        Self {
            live: core::array::from_fn(|i| {
                if C::Gap::ALL[i].is_standing() {
                    GapState::Standing
                } else {
                    GapState::Absent
                }
            }),
            hits: [0; N],
        }
    }

    pub fn raise(&mut self, cause: C) {
        let slot = cause.gap().index();
        self.hits[slot] = self.hits[slot].saturating_add(1);
        self.live[slot] = GapState::Raised(cause);
    }

    pub fn clear(&mut self, gap: C::Gap) {
        if !gap.is_standing() {
            self.live[gap.index()] = GapState::Absent;
        }
    }

    pub fn is_live(&self, gap: C::Gap) -> bool {
        !matches!(self.live[gap.index()], GapState::Absent)
    }

    pub fn cause(&self, gap: C::Gap) -> Option<&C> {
        match &self.live[gap.index()] {
            GapState::Raised(cause) => Some(cause),
            GapState::Absent | GapState::Standing => None,
        }
    }

    pub fn hits(&self, gap: C::Gap) -> u64 {
        self.hits[gap.index()]
    }

    pub fn live(&self) -> impl Iterator<Item = C::Gap> + '_ {
        C::Gap::ALL.iter().copied().filter(|g| self.is_live(*g))
    }

    pub fn count(&self) -> usize {
        self.live
            .iter()
            .filter(|s| !matches!(s, GapState::Absent))
            .count()
    }

    pub fn write_signature(&self, out: &mut impl fmt::Write) -> fmt::Result {
        for gap in self.live() {
            out.write_str(gap.name())?;
            if let Some(cause) = self.cause(gap) {
                out.write_char('=')?;
                write!(out, "{cause}")?;
            }
            out.write_char(' ')?;
        }
        Ok(())
    }
}

impl<C: GapCause, const N: usize> fmt::Debug for GapLedger<C, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GapLedger[")?;
        self.write_signature(f)?;
        f.write_char(']')
    }
}
