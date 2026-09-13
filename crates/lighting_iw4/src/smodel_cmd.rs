use crate::SmodelSurfPath;

pub const SMODEL_PRETESS_CMD_BYTES: usize = 0xc;

pub const SMODEL_CACHE_INDEX_BANK_MASK: u16 = 0xf000;

pub const SMODEL_CACHED_CMD_MAX_BYTES: usize = (crate::SMODEL_BUCKET_CAP * 2 + 9) & !3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmodelCmdKind {
    Pretess,
    Cached,

    Unchanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmodelPretessAllocError {
    CmdOverflow,
    IndexOverflow,
    MissingRun,
    Disabled,
    DeviceLost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmodelPretessAlloc {
    pub first_index: u32,
    pub index_count: u32,
    pub cmd: [u8; SMODEL_PRETESS_CMD_BYTES],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmodelCachedCmdPlan {
    pub dest: SmodelSurfPath,
    pub consumed: usize,
    pub cmd_kind: SmodelCmdKind,
    pub cmd_bytes: usize,
    pub pretess: Option<SmodelPretessAlloc>,
}

#[must_use]
pub fn smodel_cached_cmd_bytes(count: u32) -> u32 {
    count.saturating_mul(2).saturating_add(9) & !3
}

#[must_use]
pub fn smodel_same_bank_count(cache_indices: &[u16]) -> usize {
    let Some(&first) = cache_indices.first() else {
        return 0;
    };
    if first == 0 {
        return 0;
    }
    let bank_base = (first.wrapping_sub(1) & SMODEL_CACHE_INDEX_BANK_MASK).wrapping_add(1);
    let mut n = 1usize;
    for &index in &cache_indices[1..] {
        if u32::from(index).wrapping_sub(u32::from(bank_base)) > 0xfff {
            break;
        }
        n += 1;
    }
    n
}

#[must_use]
pub fn write_smodel_pretess_cmd(count: u16, first_cache_index: u16, first_index: u32) -> [u8; 12] {
    let mut bytes = [0u8; 12];
    bytes[4..6].copy_from_slice(&count.to_le_bytes());
    bytes[6..8].copy_from_slice(&first_cache_index.to_le_bytes());
    bytes[8..12].copy_from_slice(&first_index.to_le_bytes());
    bytes
}

#[must_use]
pub fn write_smodel_cached_cmd(out: &mut [u8], cache_indices: &[u16]) -> Option<usize> {
    let count = u32::try_from(cache_indices.len()).ok()?;
    let size = usize::try_from(smodel_cached_cmd_bytes(count)).ok()?;
    if out.len() < size {
        return None;
    }
    out[..size].fill(0);
    let n = u16::try_from(cache_indices.len()).ok()?;
    out[4..6].copy_from_slice(&n.to_le_bytes());
    let payload = cache_indices.len().checked_mul(2)?;
    let dst = out.get_mut(6..6 + payload)?;
    for (i, index) in cache_indices.iter().enumerate() {
        let off = i.checked_mul(2)?;
        dst.get_mut(off..off + 2)?
            .copy_from_slice(&index.to_le_bytes());
    }
    Some(size)
}

fn pretess_index_count(runs: &[&[u16]]) -> Option<u32> {
    let mut total = 0u32;
    for run in runs {
        if run.is_empty() || !run.len().is_multiple_of(3) {
            return None;
        }
        let n = u32::try_from(run.len()).ok()?;
        total = total.checked_add(n)?;
    }
    (total > 0).then_some(total)
}

pub fn smodel_cmd_reserve(
    cmd_used: usize,
    cmd_cap: usize,
    dest_used: u32,
    dest_total: u32,
    runs: &[&[u16]],
    first_cache_index: u16,
    instance_count: u16,
) -> Result<SmodelPretessAlloc, SmodelPretessAllocError> {
    if cmd_used.saturating_add(SMODEL_PRETESS_CMD_BYTES) > cmd_cap {
        return Err(SmodelPretessAllocError::CmdOverflow);
    }
    let index_count = pretess_index_count(runs).ok_or(SmodelPretessAllocError::MissingRun)?;
    let dest_end = dest_used
        .checked_add(index_count)
        .ok_or(SmodelPretessAllocError::IndexOverflow)?;
    if dest_end > dest_total {
        return Err(SmodelPretessAllocError::IndexOverflow);
    }
    Ok(SmodelPretessAlloc {
        first_index: dest_used,
        index_count,
        cmd: write_smodel_pretess_cmd(instance_count, first_cache_index, dest_used),
    })
}

pub fn smodel_pretess_indices_copy(
    dest: &mut [u16],
    alloc: SmodelPretessAlloc,
    runs: &[&[u16]],
) -> bool {
    let start = usize::try_from(alloc.first_index).ok();
    let count = usize::try_from(alloc.index_count).ok();
    let (Some(start), Some(count)) = (start, count) else {
        return false;
    };
    let Some(span) = dest.get_mut(start..start.saturating_add(count)) else {
        return false;
    };
    let mut off = 0usize;
    for run in runs {
        if off.saturating_add(run.len()) > span.len() {
            return false;
        }
        span[off..off + run.len()].copy_from_slice(run);
        off += run.len();
    }
    off == span.len()
}

#[must_use]
pub fn smodel_cached_cmd_plan(
    source: SmodelSurfPath,
    cache_indices: &[u16],
    r_pretess: bool,
    device_lost: bool,
    cmd_used: usize,
    cmd_cap: usize,
    dest_used: u32,
    dest_total: u32,
    runs: &[&[u16]],
) -> SmodelCachedCmdPlan {
    if source != SmodelSurfPath::Cached {
        return SmodelCachedCmdPlan {
            dest: source,
            consumed: cache_indices.len(),
            cmd_kind: SmodelCmdKind::Unchanged,
            cmd_bytes: 0,
            pretess: None,
        };
    }
    let consumed = smodel_same_bank_count(cache_indices);
    if consumed == 0 {
        return SmodelCachedCmdPlan {
            dest: SmodelSurfPath::Cached,
            consumed: 0,
            cmd_kind: SmodelCmdKind::Unchanged,
            cmd_bytes: 0,
            pretess: None,
        };
    }
    let batch = &cache_indices[..consumed];
    let instance_count = u16::try_from(consumed).unwrap_or(u16::MAX);
    if !device_lost && r_pretess {
        match smodel_cmd_reserve(
            cmd_used,
            cmd_cap,
            dest_used,
            dest_total,
            runs,
            batch[0],
            instance_count,
        ) {
            Ok(alloc) => {
                return SmodelCachedCmdPlan {
                    dest: SmodelSurfPath::Pretess,
                    consumed,
                    cmd_kind: SmodelCmdKind::Pretess,
                    cmd_bytes: SMODEL_PRETESS_CMD_BYTES,
                    pretess: Some(alloc),
                };
            }
            Err(SmodelPretessAllocError::Disabled | SmodelPretessAllocError::DeviceLost) => {}
            Err(_) => {}
        }
    }
    let cmd_bytes = usize::try_from(smodel_cached_cmd_bytes(instance_count as u32)).unwrap_or(0);
    if cmd_used.saturating_add(cmd_bytes) > cmd_cap {
        return SmodelCachedCmdPlan {
            dest: SmodelSurfPath::Cached,
            consumed: 0,
            cmd_kind: SmodelCmdKind::Unchanged,
            cmd_bytes: 0,
            pretess: None,
        };
    }
    SmodelCachedCmdPlan {
        dest: SmodelSurfPath::Cached,
        consumed,
        cmd_kind: SmodelCmdKind::Cached,
        cmd_bytes,
        pretess: None,
    }
}
