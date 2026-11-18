#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClientId(pub u32);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LifeSequence(pub u32);

impl LifeSequence {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandSequence(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FireCause {
    pub client: ClientId,
    pub life: LifeSequence,
    pub command: CommandSequence,
    pub ordinal: u16,
    pub hand: u8,
}

impl FireCause {
    pub fn combat_seed(self, root: u64) -> u32 {
        let mut seed = root;
        for value in [
            self.client.0,
            self.life.0,
            self.command.0,
            u32::from(self.ordinal),
            u32::from(self.hand),
        ] {
            seed = mix_root_domain(seed ^ u64::from(value), RngDomain::Combat);
        }
        MatchRng::new(seed).next_u32()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingBrass {
    pub cause: Option<FireCause>,
    pub life: LifeSequence,
    pub shot: ShotId,
    pub weapon: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShotId(pub u32);

impl ShotId {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

pub const RNG_DOMAIN_SCHEME: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum RngDomain {
    Spawn = 1,
    Combat = 2,
    Bot = 3,
}

fn mix_root_domain(root: u64, domain: RngDomain) -> u64 {
    let mut z = root
        .wrapping_add((domain as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0xA5A5_A5A5_5A5A_5A5A);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchRng {
    seed: u64,

    draws: u64,
    state: u64,
}

impl MatchRng {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            draws: 0,

            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    pub fn from_root(root: u64, domain: RngDomain) -> Self {
        Self::new(mix_root_domain(root, domain))
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn draws(&self) -> u64 {
        self.draws
    }

    pub fn restore_draws(&mut self, draws: u64) {
        self.draws = draws;
    }

    pub fn next_u32(&mut self) -> u32 {
        self.draws = self.draws.wrapping_add(1);

        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        ((x.wrapping_mul(0x2545_F491_4F6C_DD1D)) >> 32) as u32
    }

    pub fn next_index(&mut self, len: usize) -> usize {
        debug_assert!(len > 0);
        (self.next_u32() as usize) % len
    }
}
