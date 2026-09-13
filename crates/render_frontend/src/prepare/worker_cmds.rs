pub const WORKER_CMD_COUNT: usize = 21;

pub const WORKER_CMD_CELL_DYN_BRUSH: u8 = 0;

pub const WORKER_CMD_CELL_DYN_MODEL: u8 = 1;

pub const WORKER_CMD_CELL_SCENE_ENT: u8 = 2;

pub const WORKER_CMD_DPVS_ENT: u8 = 3;

pub const WORKER_CMD_BOUND_ENT: u8 = 4;

pub const WORKER_CMD_SPOT_SHADOW_ENT: u8 = 5;

pub const WORKER_CMD_SMODELCACHE: u8 = 0x11;

pub const WORKER_CMD_STRIDE: usize = 0x80;

pub const WORKER_CMD_MAX_DATA_SIZE: usize = 0x4c;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerCmdInit {
    pub data_size: u32,
    pub buf_size: u32,
}

pub const WORKER_CMD_INIT: [WorkerCmdInit; WORKER_CMD_COUNT] = [
    WorkerCmdInit {
        data_size: 0x0c,
        buf_size: 0x1800,
    },
    WorkerCmdInit {
        data_size: 0x0c,
        buf_size: 0x1800,
    },
    WorkerCmdInit {
        data_size: 0x0c,
        buf_size: 0x1800,
    },
    WorkerCmdInit {
        data_size: 0x10,
        buf_size: 0x8000,
    },
    WorkerCmdInit {
        data_size: 0x04,
        buf_size: 0x0400,
    },
    WorkerCmdInit {
        data_size: 0x08,
        buf_size: 0x0800,
    },
    WorkerCmdInit {
        data_size: 0x0c,
        buf_size: 0x0c,
    },
    WorkerCmdInit {
        data_size: 0x04,
        buf_size: 0x04,
    },
    WorkerCmdInit {
        data_size: 0x0c,
        buf_size: 0x0c,
    },
    WorkerCmdInit {
        data_size: 0x0c,
        buf_size: 0x0c00,
    },
    WorkerCmdInit {
        data_size: 0x04,
        buf_size: 0x04,
    },
    WorkerCmdInit {
        data_size: 0x18,
        buf_size: 0x1800,
    },
    WorkerCmdInit {
        data_size: 0x10,
        buf_size: 0x10,
    },
    WorkerCmdInit {
        data_size: 0x4c,
        buf_size: 0x4c,
    },
    WorkerCmdInit {
        data_size: 0x48,
        buf_size: 0x90,
    },
    WorkerCmdInit {
        data_size: 0x3c,
        buf_size: 0x78,
    },
    WorkerCmdInit {
        data_size: 0x48,
        buf_size: 0x90,
    },
    WorkerCmdInit {
        data_size: 0x04,
        buf_size: 0x0800,
    },
    WorkerCmdInit {
        data_size: 0x24,
        buf_size: 0x9000,
    },
    WorkerCmdInit {
        data_size: 0x04,
        buf_size: 0x04,
    },
    WorkerCmdInit {
        data_size: 0x08,
        buf_size: 0x18,
    },
];

#[must_use]
pub fn worker_cmd_dispatch_skips_device_lost(typ: u8) -> bool {
    (0x0d..=0x10).contains(&typ)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerCmdBusy {
    Never,
    RenderThread,
    RenderThreadOrQueued6Or7,
    Queued7,
    Queued11,
    Queued14,
    Queued11Or12OrEndFence,
    EndFence,
}

pub const WORKER_CMD_BUSY: [WorkerCmdBusy; WORKER_CMD_COUNT] = {
    use WorkerCmdBusy::*;
    [
        Never,
        Never,
        Never,
        RenderThread,
        RenderThread,
        RenderThread,
        Never,
        Never,
        RenderThreadOrQueued6Or7,
        Never,
        Never,
        Queued7,
        Queued11,
        Queued11Or12OrEndFence,
        EndFence,
        Queued14,
        EndFence,
        EndFence,
        EndFence,
        Queued7,
        Never,
    ]
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkerCmdBusyInput {
    pub is_render_thread: bool,
    pub end_fence_pending: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkinCachedStaticModelCmd {
    pub cache_index: u16,
    pub first_patch_vert: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellFrustumWorkerCmd {
    pub cell: u32,
    pub plane_count: u8,
    pub plane_begin: u8,
    pub view: u16,
}

impl CellFrustumWorkerCmd {
    #[must_use]
    pub fn to_bytes(self) -> [u8; 12] {
        let mut out = [0u8; 12];
        out[4..8].copy_from_slice(&self.cell.to_le_bytes());
        out[8] = self.plane_count;
        out[9] = self.plane_begin;
        out[10..12].copy_from_slice(&self.view.to_le_bytes());
        out
    }

    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let bytes: [u8; 12] = data.try_into().ok()?;
        Some(Self {
            cell: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            plane_count: bytes[8],
            plane_begin: bytes[9],
            view: u16::from_le_bytes([bytes[10], bytes[11]]),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DpvsEntWorkerCmd {
    pub scene_dobj: u32,
    pub plane_count: u16,

    pub cell: u16,
}

impl DpvsEntWorkerCmd {
    #[must_use]
    pub fn to_bytes(self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[0..4].copy_from_slice(&self.scene_dobj.to_le_bytes());
        out[8..10].copy_from_slice(&self.plane_count.to_le_bytes());
        out[10..12].copy_from_slice(&self.cell.to_le_bytes());
        out
    }

    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let bytes: [u8; 16] = data.try_into().ok()?;
        Some(Self {
            scene_dobj: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            plane_count: u16::from_le_bytes([bytes[8], bytes[9]]),
            cell: u16::from_le_bytes([bytes[10], bytes[11]]),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundEntWorkerCmd {
    pub scene_dobj: u32,
}

impl BoundEntWorkerCmd {
    #[must_use]
    pub fn to_bytes(self) -> [u8; 4] {
        self.scene_dobj.to_le_bytes()
    }

    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let bytes: [u8; 4] = data.try_into().ok()?;
        Some(Self {
            scene_dobj: u32::from_le_bytes(bytes),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpotShadowEntWorkerCmd {
    pub scene_ent: u32,
    pub light: u32,
}

impl SpotShadowEntWorkerCmd {
    #[must_use]
    pub fn to_bytes(self) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[..4].copy_from_slice(&self.scene_ent.to_le_bytes());
        out[4..].copy_from_slice(&self.light.to_le_bytes());
        out
    }

    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let bytes: [u8; 8] = data.try_into().ok()?;
        Some(Self {
            scene_ent: u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            light: u32::from_le_bytes(bytes[4..8].try_into().ok()?),
        })
    }
}

pub fn enqueue_cell_frustum_cmds(
    queues: &mut WorkerCmdQueues,
    cmd: CellFrustumWorkerCmd,
) -> Result<[AddWorkerCmd; 3], WorkerCmdError> {
    let data = cmd.to_bytes();
    Ok([
        queues.add(WORKER_CMD_CELL_DYN_BRUSH, &data)?,
        queues.add(WORKER_CMD_CELL_DYN_MODEL, &data)?,
        queues.add(WORKER_CMD_CELL_SCENE_ENT, &data)?,
    ])
}

impl SkinCachedStaticModelCmd {
    #[must_use]
    pub fn to_bytes(self) -> [u8; 4] {
        let mut out = [0u8; 4];
        out[..2].copy_from_slice(&self.cache_index.to_le_bytes());
        out[2..].copy_from_slice(&self.first_patch_vert.to_le_bytes());
        out
    }

    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let bytes: [u8; 4] = data.try_into().ok()?;
        Some(Self {
            cache_index: u16::from_le_bytes([bytes[0], bytes[1]]),
            first_patch_vert: u16::from_le_bytes([bytes[2], bytes[3]]),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddWorkerCmd {
    Queued,

    OverflowInline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerCmdError {
    Type,
    DataSize,
}

struct WorkerCmds {
    start_pos: i32,

    end_pos: i32,
    synced_end_pos: i32,
    in_size: i32,
    out_size: i32,
    data_size: u32,
    buf: Vec<u8>,
    buf_size: u32,
    buf_count: i32,
}

pub struct WorkerCmdQueues {
    cmds: [WorkerCmds; WORKER_CMD_COUNT],
}

impl Default for WorkerCmdQueues {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerCmdQueues {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cmds: core::array::from_fn(|i| init_cmds(WORKER_CMD_INIT[i])),
        }
    }

    #[must_use]
    pub fn data_size(&self, typ: u8) -> Option<u32> {
        self.cmds.get(usize::from(typ)).map(|c| c.data_size)
    }

    #[must_use]
    pub fn buf_count(&self, typ: u8) -> Option<i32> {
        self.cmds.get(usize::from(typ)).map(|c| c.buf_count)
    }

    #[must_use]
    pub fn in_size(&self, typ: u8) -> Option<i32> {
        self.cmds.get(usize::from(typ)).map(|c| c.in_size)
    }

    pub fn reset_type(&mut self, typ: u8) -> Result<(), WorkerCmdError> {
        let cmds = self
            .cmds
            .get_mut(usize::from(typ))
            .ok_or(WorkerCmdError::Type)?;
        reset_pos(cmds);
        Ok(())
    }

    pub fn add(&mut self, typ: u8, data: &[u8]) -> Result<AddWorkerCmd, WorkerCmdError> {
        let cmds = self
            .cmds
            .get_mut(usize::from(typ))
            .ok_or(WorkerCmdError::Type)?;
        if data.len() != cmds.data_size as usize {
            return Err(WorkerCmdError::DataSize);
        }
        let old_in = cmds.in_size;
        cmds.in_size += 1;
        if old_in < cmds.buf_count {
            let buf_size = cmds.buf_size as i32;
            let data_size = cmds.data_size as i32;
            let old_end = cmds.end_pos;
            cmds.end_pos += data_size;
            let mut pos = old_end % buf_size;
            if pos == 0 {
                cmds.end_pos -= buf_size;
            }
            if pos < 0 {
                pos += buf_size;
            }
            let n = cmds.data_size as usize;
            cmds.buf[pos as usize..pos as usize + n].copy_from_slice(data);
            let next = (data_size + pos) % buf_size;
            debug_assert_eq!(cmds.synced_end_pos, pos);
            cmds.synced_end_pos = next;
            cmds.out_size += 1;
            Ok(AddWorkerCmd::Queued)
        } else {
            cmds.in_size -= 1;

            Ok(AddWorkerCmd::OverflowInline)
        }
    }

    #[must_use]
    pub fn output_busy(&self, typ: u8, input: WorkerCmdBusyInput) -> Option<bool> {
        let idx = usize::from(typ);
        if idx >= WORKER_CMD_COUNT {
            return Some(true);
        }
        eval_busy(
            WORKER_CMD_BUSY[idx],
            |t| self.in_size(t).unwrap_or(0) > 0,
            input,
        )
    }

    pub fn process_type(
        &mut self,
        typ: u8,
        input: WorkerCmdBusyInput,
        exec: impl FnMut(&mut [u8]),
    ) -> Result<bool, WorkerCmdError> {
        match self.output_busy(typ, input) {
            None => self.process(typ, None, exec),
            Some(pending) => {
                let mut busy = move |_: &[u8]| pending;
                let busy_ref: &mut dyn FnMut(&[u8]) -> bool = &mut busy;
                self.process(typ, Some(busy_ref), exec)
            }
        }
    }

    pub fn process(
        &mut self,
        typ: u8,
        mut busy: Option<&mut dyn FnMut(&[u8]) -> bool>,
        mut exec: impl FnMut(&mut [u8]),
    ) -> Result<bool, WorkerCmdError> {
        let idx = usize::from(typ);
        if idx >= WORKER_CMD_COUNT {
            return Err(WorkerCmdError::Type);
        }
        let old_out = self.cmds[idx].out_size;
        self.cmds[idx].out_size -= 1;
        if old_out <= 0 {
            self.cmds[idx].out_size += 1;
            return Ok(false);
        }
        if busy.is_none() {
            let mut count = 1u32;
            if self.cmds[idx].out_size > 0x13 {
                let old = self.cmds[idx].out_size;
                self.cmds[idx].out_size -= 9;
                if old < 9 {
                    self.cmds[idx].out_size += 9;
                } else {
                    count = 10;
                }
            }
            let mut scratch = [0u8; WORKER_CMD_MAX_DATA_SIZE];
            let n = self.cmds[idx].data_size as usize;
            let buf_count = self.cmds[idx].buf_count;
            let mut slot = self.cmds[idx].start_pos;
            for _ in 0..count {
                copy_slot(&self.cmds[idx], slot, &mut scratch[..n]);
                exec(&mut scratch[..n]);
                slot += 1;
                if slot == buf_count {
                    slot = 0;
                }
            }
            self.cmds[idx].start_pos = slot;
            self.cmds[idx].in_size -= count as i32;
            return Ok(true);
        }
        let n = self.cmds[idx].data_size as usize;
        let mut scratch = [0u8; WORKER_CMD_MAX_DATA_SIZE];
        loop {
            let slot = self.cmds[idx].start_pos;
            copy_slot(&self.cmds[idx], slot, &mut scratch[..n]);
            if busy.as_mut().is_some_and(|b| b(&scratch[..n])) {
                self.cmds[idx].out_size += 1;
                return Ok(false);
            }
            let mut next = slot + 1;
            if next == self.cmds[idx].buf_count {
                next = 0;
            }
            if self.cmds[idx].start_pos == slot {
                self.cmds[idx].start_pos = next;
                exec(&mut scratch[..n]);
                self.cmds[idx].in_size -= 1;
                return Ok(true);
            }
        }
    }

    pub fn wait_of_type(
        &mut self,
        typ: u8,
        input: WorkerCmdBusyInput,
        mut exec: impl FnMut(&mut [u8]),
    ) -> Result<(), WorkerCmdError> {
        while self.in_size(typ).unwrap_or(0) > 0 {
            if !self.process_type(typ, input, &mut exec)? {
                break;
            }
        }
        Ok(())
    }
}

fn eval_busy(
    busy: WorkerCmdBusy,
    queued: impl Fn(u8) -> bool,
    input: WorkerCmdBusyInput,
) -> Option<bool> {
    let end_fence = input.end_fence_pending.unwrap_or(false);
    match busy {
        WorkerCmdBusy::Never => None,
        WorkerCmdBusy::RenderThread => Some(input.is_render_thread),
        WorkerCmdBusy::RenderThreadOrQueued6Or7 => {
            Some(queued(6) || queued(7) || input.is_render_thread)
        }
        WorkerCmdBusy::Queued7 => Some(queued(7)),
        WorkerCmdBusy::Queued11 => Some(queued(11)),
        WorkerCmdBusy::Queued14 => Some(queued(14)),
        WorkerCmdBusy::Queued11Or12OrEndFence => Some(queued(11) || queued(12) || end_fence),
        WorkerCmdBusy::EndFence => Some(end_fence),
    }
}

fn init_cmds(init: WorkerCmdInit) -> WorkerCmds {
    debug_assert!(init.data_size > 0);
    debug_assert!(init.buf_size % init.data_size == 0);
    debug_assert!(init.data_size as usize <= WORKER_CMD_MAX_DATA_SIZE);
    let mut cmds = WorkerCmds {
        start_pos: 0,
        end_pos: 0,
        synced_end_pos: 0,
        in_size: 0,
        out_size: 0,
        data_size: init.data_size,
        buf: vec![0; init.buf_size as usize],
        buf_size: init.buf_size,
        buf_count: (init.buf_size / init.data_size) as i32,
    };
    reset_pos(&mut cmds);
    cmds
}

fn reset_pos(cmds: &mut WorkerCmds) {
    cmds.start_pos = 0;
    cmds.end_pos = cmds.buf_size as i32;
    cmds.synced_end_pos = 0;
    cmds.in_size = 0;
    cmds.out_size = 0;
}

fn copy_slot(cmds: &WorkerCmds, slot: i32, dst: &mut [u8]) {
    let n = cmds.data_size as usize;
    let off = (slot as usize) * n;
    dst.copy_from_slice(&cmds.buf[off..off + n]);
}
