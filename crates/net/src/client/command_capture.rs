use std::collections::VecDeque;

use bevy::prelude::Resource;
use playerstate_iw4::UserCmd;
use sim::{ClientId, ShotSampleProvenance};

use super::predict::CmdSeq;

// Leave room beyond a 45 s clip for delayed snapshot delivery. Also bound memory
// when the command clock stalls (or many commands share a timestamp).
const RETAIN_MS: i32 = 60_000;
const MAX_COMMANDS: usize = 65_536;

#[derive(Clone, Copy, Debug)]
pub struct CapturedCommand {
    pub sequence: CmdSeq,
    pub cmd: UserCmd,
    pub shot_sample: ShotSampleProvenance,
}

#[derive(Resource, Default)]
pub struct LocalCommandCapture {
    pub client: Option<ClientId>,
    commands: VecDeque<CapturedCommand>,
}

impl LocalCommandCapture {
    pub fn push(
        &mut self,
        client: ClientId,
        sequence: CmdSeq,
        cmd: UserCmd,
        shot_sample: ShotSampleProvenance,
    ) {
        if self.client != Some(client)
            || self.commands.back().is_some_and(|last| {
                sequence <= last.sequence || cmd.server_time < last.cmd.server_time
            })
        {
            self.commands.clear();
        }
        self.client = Some(client);
        self.commands.push_back(CapturedCommand {
            sequence,
            cmd,
            shot_sample,
        });
        while self.commands.len() > MAX_COMMANDS
            || self.commands.front().is_some_and(|first| {
                cmd.server_time.saturating_sub(first.cmd.server_time) > RETAIN_MS
            })
        {
            self.commands.pop_front();
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &CapturedCommand> {
        self.commands.iter()
    }
}
