use net::{CmdSeq, LocalCommandCapture};
use replay::ClipRing;
use serde_json::{Value, json};

pub(super) const FILE_NAME: &str = "usercmds.json";

pub(super) fn export(
    ring: &ClipRing,
    capture: Option<&LocalCommandCapture>,
) -> Result<String, String> {
    let client = capture.and_then(|capture| capture.client);
    let mut anchors = Vec::new();
    let mut bounds = Vec::new();
    for frame in ring.frames() {
        let frame = frame.map_err(|error| error.to_string())?;
        let ack = client.and_then(|client| {
            frame
                .acks
                .iter()
                .find(|(id, _)| *id == client)
                .map(|(_, seq)| seq.0)
        });
        anchors.push(json!({"tick": frame.tick.0, "ack_sequence": ack}));
        bounds.push(ack);
    }
    let baseline_index = bounds.iter().position(Option::is_some);
    let baseline = baseline_index.and_then(|index| bounds[index]);
    let end = bounds.iter().rev().find_map(|ack| *ack);
    let selected: Vec<_> = capture
        .into_iter()
        .flat_map(LocalCommandCapture::iter)
        .filter(|entry| {
            baseline.zip(end).is_some_and(|(first, last)| {
                entry.sequence > CmdSeq(first) && entry.sequence <= CmdSeq(last)
            })
        })
        .collect();
    let complete = baseline.zip(end).is_some_and(|(first, last)| {
        last >= first
            && selected.len() as u64 == u64::from(last - first)
            && selected
                .iter()
                .enumerate()
                .all(|(i, entry)| u64::from(entry.sequence.0) == u64::from(first) + i as u64 + 1)
    });
    let commands: Vec<Value> = selected
        .iter()
        .map(|entry| {
            let cmd = entry.cmd;
            json!({
                "sequence": entry.sequence.0,
                "server_time": cmd.server_time,
                "buttons": cmd.buttons,
                "angles": cmd.angles,
                "gun_angle_offset": cmd.gun_angle_offset,
                "weapon": cmd.weapon,
                "weapon_mapped": cmd.weapon_mapped,
                "off_hand_index": cmd.off_hand_index,
                "forwardmove": cmd.forwardmove,
                "rightmove": cmd.rightmove,
                "melee_charge_yaw": cmd.melee_charge_yaw,
                "melee_charge_dist": cmd.melee_charge_dist,
                "selected_location": cmd.selected_location,
                "remote_control": cmd.remote_control,
                "canonical_bytes": cmd.canonical_bytes().as_slice(),
                "shot_sample": {
                    "left": entry.shot_sample.left.0,
                    "right": entry.shot_sample.right.0,
                    "alpha": entry.shot_sample.alpha,
                    "quality": entry.shot_sample.quality.dump_label(),
                },
            })
        })
        .collect();
    serde_json::to_string_pretty(&json!({
        "version": 1,
        "source": "generated_local",
        "client": client.map(|client| client.0),
        "complete": complete,
        "status": if client.is_none() { "no_local_commands" } else if baseline.is_none() || end.is_none() { "missing_ack_bounds" } else if !complete { "command_history_gap" } else { "complete" },
        "baseline": {"demo": "clip.iw4ldemo", "tick": baseline_index.map(|i| &anchors[i]["tick"]), "ack_sequence": baseline},
        "end_ack_sequence": end,
        "anchors": anchors,
        "commands": commands,
    })).map_err(|error| error.to_string())
}
