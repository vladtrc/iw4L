use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub time_ms: i32,
    pub buttons: u32,
    pub angles: [i32; 3],
    pub weapon: u16,
    pub weapon_mapped: u16,
    pub off_hand_index: u16,
    pub forwardmove: i8,
    pub rightmove: i8,
    #[serde(default)]
    pub remote_control: [u8; 2],
}
impl Sample {
    pub fn neutral(time_ms: i32, look: [f64; 2], weapon: u16) -> Self {
        Self {
            time_ms,
            buttons: 0,
            angles: [angle_units(look[0]), angle_units(look[1]), 0],
            weapon,
            weapon_mapped: 0,
            off_hand_index: 0,
            forwardmove: 0,
            rightmove: 0,
            remote_control: [0; 2],
        }
    }
    pub fn canonical(&self, base: i32) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(46);
        bytes.extend((self.time_ms + base).to_le_bytes());
        bytes.extend(self.buttons.to_le_bytes());
        for v in self.angles {
            bytes.extend(v.to_le_bytes());
        }
        for v in [self.weapon, self.weapon_mapped, self.off_hand_index] {
            bytes.extend(v.to_le_bytes());
        }
        bytes.extend([self.forwardmove as u8, self.rightmove as u8]);
        bytes.extend([0; 8]);
        bytes.extend(self.remote_control);
        bytes.extend([0; 8]);
        bytes
    }
}
pub fn angle_units(degrees: f64) -> i32 {
    (degrees * 65536. / 360.).round_ties_even() as i32
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tape {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scoreboard_window_ms: Option<[i32; 2]>,
    pub quantum_ms: i32,
    pub first_sequence: u32,
    pub samples: Vec<Sample>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub future_weapons: Vec<u16>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stop_on_death: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActorTape {
    pub client: u32,
    #[serde(flatten)]
    pub tape: Tape,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Program {
    pub schema_version: u32,
    #[serde(flatten)]
    pub tape: Tape,
    #[serde(default)]
    pub actors: Vec<ActorTape>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub start_ms: i32,
    pub duration_ms: i32,
    #[serde(default)]
    pub movement: [i8; 2],
    #[serde(default)]
    pub buttons: Vec<String>,
    pub look: Option<[f64; 2]>,
    pub weapon: Option<String>,
    #[serde(default)]
    pub note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Turn {
    pub start_ms: i32,
    pub duration_ms: i32,
    pub look: [f64; 2],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    pub name: String,
    pub client: u32,
    pub weapon: String,
    #[serde(default)]
    pub look: [f64; 2],
    #[serde(default)]
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub turns: Vec<Turn>,
    #[serde(default)]
    pub future_weapons: Vec<String>,
    #[serde(default)]
    pub stop_on_death: bool,
    #[serde(skip)]
    cursor: i32,
}
impl Actor {
    pub fn new(name: impl Into<String>, client: u32, weapon: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            client,
            weapon: weapon.into(),
            look: [0.; 2],
            segments: vec![],
            turns: vec![],
            future_weapons: vec![],
            stop_on_death: false,
            cursor: 0,
        }
    }
    pub fn at(&mut self, ms: i32) -> &mut Self {
        self.cursor = ms;
        self
    }
    pub fn segment(
        &mut self,
        ms: i32,
        movement: [i8; 2],
        buttons: &[&str],
        look: Option<[f64; 2]>,
        weapon: Option<&str>,
    ) -> &mut Self {
        self.segments.push(Segment {
            start_ms: self.cursor,
            duration_ms: ms,
            movement,
            buttons: buttons.iter().map(|s| s.to_string()).collect(),
            look,
            weapon: weapon.map(str::to_owned),
            note: String::new(),
        });
        self.cursor += ms;
        self
    }
    pub fn wait(&mut self, ms: i32) -> &mut Self {
        self.segment(ms, [0, 0], &[], None, None)
    }
    pub fn press(&mut self, button: &str, ms: i32) -> &mut Self {
        self.segment(ms, [0, 0], &[button], None, None)
    }
    pub fn select(&mut self, weapon: &str, ms: i32) -> &mut Self {
        self.segment(ms, [0, 0], &[], None, Some(weapon))
    }
    pub fn yy(&mut self, other: &str, back: &str, away: i32, returning: i32) -> &mut Self {
        self.select(other, away).select(back, returning)
    }
    pub fn turn(&mut self, ms: i32, look: [f64; 2]) -> &mut Self {
        self.turns.push(Turn {
            start_ms: self.cursor,
            duration_ms: ms,
            look,
        });
        self
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bindings {
    pub weapons: BTreeMap<String, u16>,
    pub actors: BTreeMap<String, ActorBinding>,
    #[serde(default)]
    pub worlds: BTreeMap<String, u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorBinding {
    pub delta_angles: [f64; 3],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub name: String,
    pub duration_ms: i32,
    pub quantum_ms: i32,
    pub actors: Vec<Actor>,
    #[serde(default)]
    pub beats: BTreeMap<String, i32>,
    #[serde(default)]
    pub expectations: Vec<Value>,
}
pub fn button(name: &str) -> Result<u32> {
    Ok(match name {
        "attack" => 1,
        "sprint" => 2,
        "melee" => 4,
        "use" => 8,
        "reload" => 16,
        "use_reload" => 32,
        "prone" => 0x100,
        "crouch" => 0x200,
        "jump" => 0x400,
        "ads" => 0x800,
        "breath" => 0x2000,
        "frag" => 0x4000,
        "smoke" => 0x8000,
        "throw" => 0x80000,
        "remote" => 0x100000,
        _ => return Err(format!("unknown button: {name}").into()),
    })
}
impl Scene {
    pub fn compile(&self, bindings: &Bindings) -> Result<(Program, Value)> {
        let q = self.quantum_ms;
        if q <= 0
            || 50 % q != 0
            || !(50..=120000).contains(&self.duration_ms)
            || self.duration_ms % 50 != 0
        {
            return Err("invalid take duration or quantum".into());
        }
        if self.actors.first().map(|a| a.client) != Some(0) {
            return Err("first actor must be local client 0".into());
        }
        let mut names = BTreeSet::new();
        let mut clients = BTreeSet::new();
        for actor in &self.actors {
            if !names.insert(&actor.name) || !clients.insert(actor.client) {
                return Err("duplicate actor name/client".into());
            }
        }
        for time in self.beats.values() {
            if *time < 0 || *time >= self.duration_ms || time % q != 0 {
                return Err("beat outside take or off quantum".into());
            }
        }
        let resolve = |s: &str| -> Result<u16> {
            if s.is_empty() {
                return Ok(0);
            }
            bindings
                .weapons
                .get(s)
                .copied()
                .filter(|v| *v > 0)
                .ok_or_else(|| format!("missing/invalid weapon binding: {s}").into())
        };
        let mut tapes = Vec::new();
        for a in &self.actors {
            let delta = bindings
                .actors
                .get(&a.name)
                .ok_or_else(|| format!("missing actor binding: {}", a.name))?
                .delta_angles;
            if delta.iter().chain(a.look.iter()).any(|v| !v.is_finite()) {
                return Err("nonfinite look/basis".into());
            }
            let mut segments = a.segments.clone();
            segments.sort_by_key(|r| r.start_ms);
            let mut end = 0;
            let mut turns = a.turns.clone();
            for r in &segments {
                validate_span(r.start_ms, r.duration_ms, end, self.duration_ms, q)?;
                end = r.start_ms + r.duration_ms;
                if r.movement.contains(&-128) {
                    return Err("movement outside -127..127".into());
                }
                for b in &r.buttons {
                    button(b)?;
                }
                if let Some(look) = r.look {
                    turns.push(Turn {
                        start_ms: r.start_ms,
                        duration_ms: r.duration_ms,
                        look,
                    });
                }
            }
            turns.sort_by_key(|r| r.start_ms);
            end = 0;
            for r in &turns {
                validate_span(r.start_ms, r.duration_ms, end, self.duration_ms, q)?;
                end = r.start_ms + r.duration_ms;
                if r.look.iter().any(|v| !v.is_finite()) {
                    return Err("nonfinite turn".into());
                }
            }
            let mut samples = Vec::new();
            let mut weapon = resolve(&a.weapon)?;
            let mut look = a.look;
            let mut ti = 0;
            let mut si = 0;
            for t in (q..=self.duration_ms).step_by(q as usize) {
                while si < segments.len() && t > segments[si].start_ms + segments[si].duration_ms {
                    si += 1;
                }
                let row = segments.get(si).filter(|r| t > r.start_ms);
                if let Some(r) = row
                    && let Some(w) = &r.weapon
                {
                    weapon = resolve(w)?;
                }
                while ti < turns.len() && t > turns[ti].start_ms + turns[ti].duration_ms {
                    look = turns[ti].look;
                    ti += 1;
                }
                let mut view = look;
                if let Some(r) = turns.get(ti).filter(|r| t > r.start_ms) {
                    let f = f64::from(t - r.start_ms) / f64::from(r.duration_ms);
                    for i in 0..2 {
                        view[i] = look[i] + (r.look[i] - look[i]) * f;
                    }
                }
                let mut s = Sample::neutral(t, [view[0] - delta[0], view[1] - delta[1]], weapon);
                s.angles[2] = angle_units(-delta[2]);
                if let Some(r) = row {
                    s.forwardmove = r.movement[0];
                    s.rightmove = r.movement[1];
                    for b in &r.buttons {
                        s.buttons |= button(b)?;
                    }
                }
                samples.push(s);
            }
            tapes.push(Tape {
                scoreboard_window_ms: None,
                quantum_ms: q,
                first_sequence: 1,
                samples,
                future_weapons: a
                    .future_weapons
                    .iter()
                    .map(|w| resolve(w))
                    .collect::<Result<_>>()?,
                stop_on_death: a.stop_on_death,
            });
        }
        let program = Program {
            schema_version: 2,
            tape: tapes.remove(0),
            actors: self
                .actors
                .iter()
                .skip(1)
                .zip(tapes)
                .map(|(a, tape)| ActorTape {
                    client: a.client,
                    tape,
                })
                .collect(),
        };
        let value = serde_json::to_value(&program)?;
        let manifest = json!({"schema_version":2,"scene":self.name,"duration_ms":self.duration_ms,"program_json_sha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&value)?)),"bindings":bindings,"beats_ms":self.beats,"expectations":self.expectations,"actors":self.actors});
        Ok((program, manifest))
    }
}
fn validate_span(start: i32, duration: i32, previous: i32, end: i32, q: i32) -> Result<()> {
    if start < previous
        || duration <= 0
        || start.checked_add(duration).is_none_or(|v| v > end)
        || start % q != 0
        || duration % q != 0
    {
        return Err("overlapping, off-quantum or out-of-range span".into());
    }
    Ok(())
}
pub fn tape_digest(tape: &Tape, client: u32) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"iw4l-input-program-v1\0");
    h.update(client.to_le_bytes());
    h.update(tape.quantum_ms.to_le_bytes());
    h.update(tape.first_sequence.to_le_bytes());
    if tape.stop_on_death || !tape.future_weapons.is_empty() {
        h.update(b"lifecycle-v1\0");
        h.update([u8::from(tape.stop_on_death)]);
        h.update((tape.future_weapons.len() as u32).to_le_bytes());
        for w in &tape.future_weapons {
            h.update(w.to_le_bytes());
        }
    }
    if let Some([start, end]) = tape.scoreboard_window_ms {
        h.update(b"scoreboard-v1\0");
        h.update(start.to_le_bytes());
        h.update(end.to_le_bytes());
    }
    for s in &tape.samples {
        h.update(s.canonical(0));
    }
    h.finalize().into()
}
