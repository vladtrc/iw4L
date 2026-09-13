use bevy::prelude::*;
use gamemode_iw4::{PARSE_SCORES_CAP, ParsedScores, parse_scores};
use sim::{ClientId, ClientSnapshotMeta, Snapshot};

use crate::transport::wire::{WireError, WireReader, WireWriter};

pub const SVC_SCORES: u8 = b'b';

pub const SCORES_REQUEST_MS: i32 = 2000;

#[derive(Resource, Clone, Debug, Default)]
pub struct CgScores {
    pub cmd: Option<String>,
    pub parsed: ParsedScores,
}

#[derive(Resource, Debug)]
pub struct PendingScoreboard {
    last_sent_ms: i32,
    last_broadcast_ms: i32,
}

impl Default for PendingScoreboard {
    fn default() -> Self {
        Self {
            last_sent_ms: -1,
            last_broadcast_ms: -1,
        }
    }
}

impl PendingScoreboard {
    pub fn broadcast_due(&mut self, now_ms: i32) -> bool {
        if self.last_broadcast_ms < 0 || self.last_broadcast_ms + SCORES_REQUEST_MS < now_ms {
            self.last_broadcast_ms = now_ms;
            true
        } else {
            false
        }
    }

    pub fn take_if_due(&mut self, scores_down: bool, now_ms: i32) -> bool {
        if !scores_down {
            self.last_sent_ms = -1;
            return false;
        }
        if self.last_sent_ms < 0 || self.last_sent_ms + SCORES_REQUEST_MS < now_ms {
            self.last_sent_ms = now_ms;
            true
        } else {
            false
        }
    }
}

pub fn format_scoreboard_cmd(
    clients: &[(ClientId, &ClientSnapshotMeta)],
    team_axis: i32,
    team_allies: i32,
    score_limit: i32,
) -> String {
    let n = clients.len().min(PARSE_SCORES_CAP);
    let mut entries = String::new();
    for (id, meta) in clients.iter().take(n) {
        entries.push_str(&format!(
            " {} {} {} {} {} {} {} {}",
            id.0, meta.score, 0, meta.deaths, 0, meta.kills, 0, 0
        ));
    }
    format!("b {n} {team_axis} {team_allies} {score_limit}{entries}")
}

pub fn format_scoreboard_from_snapshot(snap: &Snapshot) -> String {
    let mut packed: Vec<(ClientId, &ClientSnapshotMeta)> = snap
        .meta
        .clients
        .iter()
        .map(|(id, meta)| (*id, meta))
        .collect();
    packed.sort_by(|a, b| {
        b.1.score
            .cmp(&a.1.score)
            .then_with(|| a.1.deaths.cmp(&b.1.deaths))
    });
    packed.truncate(PARSE_SCORES_CAP);

    let team_axis = snap.meta.objectives.scores.get(1).copied().unwrap_or(0);
    let team_allies = snap.meta.objectives.scores.get(2).copied().unwrap_or(0);
    format_scoreboard_cmd(&packed, team_axis, team_allies, snap.meta.score_limit)
}

pub fn parse_scoreboard_cmd(cmd: &str) -> ParsedScores {
    let argv: Vec<&str> = cmd.split_whitespace().collect();
    parse_scores(&argv)
}

pub fn encode_svc_scores(out: &mut WireWriter, cmd: Option<&str>) {
    match cmd {
        None => out.put_u8(0),
        Some(s) => {
            debug_assert!(s.len() <= u16::MAX as usize);
            out.put_u8(1);
            out.put_u16(s.len() as u16);
            out.put_bytes(s.as_bytes());
        }
    }
}

pub fn decode_svc_scores(input: &mut WireReader<'_>) -> Result<Option<String>, WireError> {
    match input.get_u8()? {
        0 => Ok(None),
        1 => {
            let len = input.get_u16()? as usize;
            let mut bytes = vec![0; len];
            input.get_bytes(&mut bytes)?;
            String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| WireError::Malformed("svc 'b' is not UTF-8"))
        }
        _ => Err(WireError::Malformed("svc scores tag is not 0/1")),
    }
}
