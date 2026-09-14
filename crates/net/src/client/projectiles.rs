use entity_iw4::Trajectory;
use sim::{ClientId, ProjectileId, ProjectileState};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PresentedProjectile {
    Authoritative(ProjectileState),

    Predicted {
        owner: ClientId,
        weapon: u32,
        origin: [f32; 3],
        velocity: [f32; 3],
        pos: Trajectory,
        apos: Trajectory,
        launch_time: i32,
    },
}

pub fn merge_presented_projectiles(
    authoritative: &[ProjectileState],
    predicted: &[ProjectileState],
    local: ClientId,
) -> Vec<PresentedProjectile> {
    let mut out: Vec<PresentedProjectile> = authoritative
        .iter()
        .copied()
        .map(PresentedProjectile::Authoritative)
        .collect();

    let authoritative_local_weapons = authoritative
        .iter()
        .filter(|p| p.owner == local)
        .map(|p| p.weapon)
        .collect::<std::collections::HashSet<_>>();

    for projectile in predicted.iter().filter(|p| p.owner == local) {
        if authoritative_local_weapons.contains(&projectile.weapon) {
            continue;
        }
        out.push(PresentedProjectile::Predicted {
            owner: projectile.owner,
            weapon: projectile.weapon,
            origin: projectile.origin,
            velocity: projectile.velocity,
            pos: projectile.pos,
            apos: projectile.apos,
            launch_time: projectile.launch_time,
        });
    }

    out
}

impl PresentedProjectile {
    pub fn owner(&self) -> ClientId {
        match self {
            Self::Authoritative(p) => p.owner,
            Self::Predicted { owner, .. } => *owner,
        }
    }

    pub fn weapon(&self) -> u32 {
        match self {
            Self::Authoritative(p) => p.weapon,
            Self::Predicted { weapon, .. } => *weapon,
        }
    }

    pub fn origin(&self) -> [f32; 3] {
        match self {
            Self::Authoritative(p) => p.origin,
            Self::Predicted { origin, .. } => *origin,
        }
    }

    pub fn origin_at(&self, at_time_ms: i32) -> [f32; 3] {
        entity_iw4::bg_evaluate_trajectory(&self.pos(), at_time_ms)
    }

    pub fn velocity(&self) -> [f32; 3] {
        match self {
            Self::Authoritative(p) => p.velocity,
            Self::Predicted { velocity, .. } => *velocity,
        }
    }

    pub fn apos(&self) -> Trajectory {
        match self {
            Self::Authoritative(p) => p.apos,
            Self::Predicted { apos, .. } => *apos,
        }
    }

    pub fn pos(&self) -> Trajectory {
        match self {
            Self::Authoritative(p) => p.pos,
            Self::Predicted { pos, .. } => *pos,
        }
    }

    pub fn launch_time(&self) -> i32 {
        match self {
            Self::Authoritative(p) => p.launch_time,
            Self::Predicted { launch_time, .. } => *launch_time,
        }
    }

    pub fn authoritative_id(&self) -> Option<ProjectileId> {
        match self {
            Self::Authoritative(p) => Some(p.id),
            Self::Predicted { .. } => None,
        }
    }
}

pub fn count_throw_rows(presented: &[PresentedProjectile], owner: ClientId, weapon: u32) -> usize {
    presented
        .iter()
        .filter(|row| match row {
            PresentedProjectile::Authoritative(p) => p.owner == owner && p.weapon == weapon,
            PresentedProjectile::Predicted {
                owner: o,
                weapon: w,
                ..
            } => *o == owner && *w == weapon,
        })
        .count()
}
