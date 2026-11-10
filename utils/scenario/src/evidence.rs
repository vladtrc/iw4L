use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RunReport {
    pub client: i64,
    pub complete: bool,
    pub failure: Option<Value>,
    pub program_sha256: String,
    pub ensemble_sha256: String,
    pub base_command_time: Option<i64>,
    pub death_tick: Option<i64>,
    pub initial_origin: [f64; 3],
    pub actors: Option<Vec<RunReport>>,
    pub receipts: Option<Vec<Receipt>>,
    pub events: Option<Vec<EventRow>>,
    pub shots: Option<Vec<ShotRow>>,
    pub launches: Option<Vec<LaunchRow>>,
    pub shot_verdicts: Option<Vec<VerdictRow>>,
    pub pickups: Option<Vec<PickupRow>>,
    pub initial_streak: Streak,
    pub final_streak: Streak,
    pub initial_match: MatchInfo,
    pub final_match: FinalMatch,
    pub match_states: Option<Vec<MatchState>>,
    pub final_killcams: Option<Vec<Killcam>>,
    pub scoreboard_samples: Option<Vec<ScoreboardSample>>,
    pub shot_scenes: Option<Vec<ShotScene>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Receipt {
    pub tick: Option<i64>,
    pub sequence: Option<i64>,
    pub server_time: Option<i64>,
    pub origin: [f64; 3],
    pub velocity: [f64; 3],
    pub angles: [f64; 3],
    pub world: Option<i64>,
    pub health: Option<i64>,
    pub weapon: Option<i64>,
    pub weapon_state: Option<i64>,
    pub weapon_anim: Option<i64>,
    pub ground_entity: Option<i64>,
    pub inventory: Vec<i64>,
    pub weapon_data: Vec<u64>,
    pub ads_frac: Option<f64>,
    pub bytes: Vec<u8>,
    pub source_bytes: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct EventRow {
    pub tick: Option<i64>,
    pub sequence: Option<i64>,
    pub data: EventData,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct EventData {
    pub kind: String,
    pub attacker: Option<i64>,
    pub victim: Option<i64>,
    pub weapon: Option<i64>,
    pub source: Option<String>,
    pub client: Option<i64>,
    pub score: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ShotRow {
    pub id: Option<i64>,
    pub weapon: Option<i64>,
    pub origin: [f64; 3],
    pub ads_frac: Option<f64>,
    pub cause: Option<Cause>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Cause {
    pub command: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct LaunchRow {
    pub weapon: Option<i64>,
    pub spawn_time_ms: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct VerdictRow {
    pub shot: Option<i64>,
    pub terminal: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct PickupRow {
    pub tick: Option<i64>,
    pub picker: Option<i64>,
    pub weapon: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Streak {
    pub count: Option<i64>,
    pub last_earned: Option<String>,
    pub owned: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct MatchInfo {
    pub score: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct FinalMatch {
    pub mode: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct MatchState {
    pub tick: Option<i64>,
    pub ended: bool,
    pub score_limit: Option<i64>,
    pub scores: Vec<SeatScore>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SeatScore {
    pub client: i64,
    pub score: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Killcam {
    pub client: i64,
    pub target: Option<i64>,
    pub start_tick: Option<i64>,
    pub last_tick: Option<i64>,
    pub closed_tick: Option<i64>,
    pub length_ms: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ScoreboardSample {
    pub tick: Option<i64>,
    pub origin: [f64; 3],
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ShotScene {
    pub tick: Option<i64>,
    pub shot: Option<i64>,
    pub targets: Vec<SceneTarget>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SceneTarget {
    pub client: i64,
    pub health: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Manifest {
    pub program_json_sha256: String,
    pub actors: Vec<NamedActor>,
    pub bindings: Bindings,
    pub beats_ms: BTreeMap<String, i64>,
    pub expectations: Vec<Expectation>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct NamedActor {
    pub name: String,
    pub client: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Bindings {
    pub weapons: BTreeMap<String, i64>,
    pub worlds: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Expectation {
    pub kind: String,
    pub actor: Option<String>,
    pub weapon: Option<String>,
    pub source: Option<String>,
    pub destination: Option<String>,
    pub weapons: Option<Vec<String>>,
    pub consume_in: Option<String>,
    pub beat: Option<String>,
    pub window_ms: Option<i64>,
}

pub fn parse_run(d: &Value) -> crate::Result<RunReport> {
    serde_json::from_value(d.clone()).map_err(|e| format!("evidence: {e}").into())
}

pub fn parse_manifest(d: &Value) -> crate::Result<Manifest> {
    serde_json::from_value(d.clone()).map_err(|e| format!("manifest: {e}").into())
}

pub fn rows<'a, T>(v: &'a Option<Vec<T>>, key: &str) -> crate::Result<&'a [T]> {
    v.as_deref()
        .ok_or_else(|| format!("missing evidence array: {key}").into())
}
