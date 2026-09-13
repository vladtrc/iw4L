use bevy::prelude::*;
use sim::ClientId;

use crate::transport::wire::{WireError, WireReader, WireWriter};

pub const SVC_DISCONNECT_NOTIFY: u8 = 0x65;

pub const SVC_PRINT: u8 = 0x66;

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct SvcGameNotify {
    pub id: u32,

    pub tag: u8,

    pub name: String,

    pub key: String,
}

#[derive(Resource, Debug, Default)]
pub struct PendingGameNotify {
    next_id: u32,
    rows: Vec<SvcGameNotify>,
}

impl PendingGameNotify {
    pub fn push_connected(&mut self, name: String) {
        self.push(SVC_PRINT, name, hud_iw4::MP_CONNECTED);
    }

    pub fn push_left(&mut self, name: String) {
        self.push(SVC_DISCONNECT_NOTIFY, name, hud_iw4::EXE_LEFTGAME);
    }

    fn push(&mut self, tag: u8, name: String, key: &'static str) {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.rows.push(SvcGameNotify {
            id,
            tag,
            name,
            key: key.to_owned(),
        });
    }

    pub fn take_broadcast(&self) -> Vec<SvcGameNotify> {
        self.rows.clone()
    }

    pub fn clear_after_fanout(&mut self) {
        self.rows.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

pub fn encode_svc_game_notifies(out: &mut WireWriter, commands: &[SvcGameNotify]) {
    out.put_u16(u16::try_from(commands.len()).expect("gamenotify count"));
    for cmd in commands {
        out.put_u32(cmd.id);
        out.put_u8(cmd.tag);
        let name = cmd.name.as_bytes();
        debug_assert!(name.len() <= u8::MAX as usize);
        out.put_u8(name.len() as u8);
        out.put_bytes(name);
        let key = cmd.key.as_bytes();
        debug_assert!(key.len() <= u8::MAX as usize);
        out.put_u8(key.len() as u8);
        out.put_bytes(key);
    }
}

pub fn decode_svc_game_notifies(
    input: &mut WireReader<'_>,
) -> Result<Vec<SvcGameNotify>, WireError> {
    if input.remaining() == 0 {
        return Ok(Vec::new());
    }
    let count = input.get_u16()? as usize;
    let mut commands = Vec::with_capacity(count);
    for _ in 0..count {
        let id = input.get_u32()?;
        let tag = input.get_u8()?;
        if tag != SVC_DISCONNECT_NOTIFY && tag != SVC_PRINT {
            return Err(WireError::Malformed("gamenotify tag is not 'e' or 'f'"));
        }
        let mut name = vec![0; input.get_u8()? as usize];
        input.get_bytes(&mut name)?;
        let name =
            String::from_utf8(name).map_err(|_| WireError::Malformed("gamenotify name UTF-8"))?;
        let mut key = vec![0; input.get_u8()? as usize];
        input.get_bytes(&mut key)?;
        let key =
            String::from_utf8(key).map_err(|_| WireError::Malformed("gamenotify key UTF-8"))?;
        commands.push(SvcGameNotify { id, tag, name, key });
    }
    Ok(commands)
}

pub fn client_name_string(world: &sim::SimWorld, id: ClientId) -> String {
    entity_iw4::client_state_name(&world.packed_client_name(id))
        .unwrap_or("")
        .to_owned()
}
