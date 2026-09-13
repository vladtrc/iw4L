use super::gpu_contract::WgpuPassLayout;
use super::material_runtime::{
    ExecutablePassView, PackedCodeConstantLane, PackedLocalBanks, PortId, RuntimeProgramPort,
    RuntimeShaderStage,
};
use super::sm3_abi::{ConstantBinding, ConstantSource};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassConstantBuffers {
    pub vertex: Vec<[u32; 4]>,
    pub pixel: Vec<[u32; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstantPackRefusal {
    ExecutablePairMismatch,

    MissingLocalBanks,
    DestinationPastConstantBlock {
        stage: RuntimeShaderStage,
        destination: u16,
        block_len: usize,
    },
    MissingConstantRegister {
        stage: RuntimeShaderStage,
        register: u16,
        source: ConstantSource,
    },
}

impl RuntimeProgramPort {
    pub fn pack_hit(
        &self,
        pass: ExecutablePassView<'_>,
    ) -> Result<PassConstantBuffers, ConstantPackRefusal> {
        let Some(seed) = pass.local_banks.map(Arc::as_ref) else {
            return Err(ConstantPackRefusal::MissingLocalBanks);
        };
        if seed.vertex.len() == self.module().vertex_constant_len
            && seed.pixel.len() == self.module().pixel_constant_len
            && seed.written_vertex.len() == seed.vertex.len()
            && seed.written_pixel.len() == seed.pixel.len()
        {
            return pack_code_on_seed(
                self.id(),
                &self.abi().vertex_constants,
                &self.abi().pixel_constants,
                seed,
                pass,
            );
        }
        Err(ConstantPackRefusal::DestinationPastConstantBlock {
            stage: RuntimeShaderStage::Vertex,
            destination: 0,
            block_len: seed.vertex.len(),
        })
    }

    pub fn pack_hit_into(
        &self,
        pass: ExecutablePassView<'_>,
        overlay: &[PackedCodeConstantLane],
        out: &mut Vec<u8>,
    ) -> Result<(), ConstantPackRefusal> {
        let Some(seed) = pass.local_banks.map(Arc::as_ref) else {
            return Err(ConstantPackRefusal::MissingLocalBanks);
        };
        let module = self.module();
        if seed.vertex.len() != module.vertex_constant_len
            || seed.pixel.len() != module.pixel_constant_len
            || seed.written_vertex.len() != seed.vertex.len()
            || seed.written_pixel.len() != seed.pixel.len()
        {
            return Err(ConstantPackRefusal::DestinationPastConstantBlock {
                stage: RuntimeShaderStage::Vertex,
                destination: 0,
                block_len: seed.vertex.len(),
            });
        }
        if !self.accepts_pass(pass) {
            return Err(ConstantPackRefusal::ExecutablePairMismatch);
        }
        require_abi_rows_with_lanes(
            RuntimeShaderStage::Vertex,
            &self.abi().vertex_constants,
            &seed.written_vertex,
            pass.code_constants,
        )?;
        require_abi_rows_with_lanes(
            RuntimeShaderStage::Pixel,
            &self.abi().pixel_constants,
            &seed.written_pixel,
            pass.code_constants,
        )?;
        let start = out.len();
        out.extend_from_slice(bytemuck::cast_slice(seed.vertex.as_slice()));
        out.extend_from_slice(bytemuck::cast_slice(seed.pixel.as_slice()));
        let result = pass
            .code_constants
            .iter()
            .chain(overlay)
            .try_for_each(|lane| {
                overlay_lane_bytes(lane, &mut out[start..], seed.vertex.len(), seed.pixel.len())
            });
        if result.is_err() {
            out.truncate(start);
        }
        result
    }
}

fn overlay_lane_bytes(
    lane: &PackedCodeConstantLane,
    span: &mut [u8],
    vertex_len: usize,
    pixel_len: usize,
) -> Result<(), ConstantPackRefusal> {
    let (bank_off, bank_len) = match lane.stage {
        RuntimeShaderStage::Vertex => (0usize, vertex_len),
        RuntimeShaderStage::Pixel => (vertex_len, pixel_len),
    };
    let first = usize::from(lane.first_row);
    let rows = &lane.rows[first..first.saturating_add(usize::from(lane.row_count))];
    for (row, words) in rows.iter().enumerate() {
        let register = usize::from(lane.destination).saturating_add(row);
        if register >= bank_len {
            return Err(ConstantPackRefusal::DestinationPastConstantBlock {
                stage: lane.stage,
                destination: u16::try_from(register).unwrap_or(u16::MAX),
                block_len: bank_len,
            });
        }
        let at = (bank_off + register) * 16;
        span[at..at + 16].copy_from_slice(bytemuck::bytes_of(words));
    }
    Ok(())
}

fn require_abi_rows_with_lanes(
    stage: RuntimeShaderStage,
    bindings: &[ConstantBinding],
    seed_written: &[bool],
    lanes: &[PackedCodeConstantLane],
) -> Result<(), ConstantPackRefusal> {
    let mut covered = [0u64; 4];
    for lane in lanes.iter().filter(|lane| lane.stage == stage) {
        let first = usize::from(lane.destination);
        for register in first..first.saturating_add(usize::from(lane.row_count)).min(256) {
            covered[register / 64] |= 1u64 << (register % 64);
        }
    }
    for binding in bindings {
        if binding.source == ConstantSource::ProgramDefined {
            continue;
        }
        let index = usize::from(binding.register);
        if index >= seed_written.len() || seed_written[index] {
            continue;
        }
        if index >= 256 || covered[index / 64] & (1u64 << (index % 64)) == 0 {
            return Err(ConstantPackRefusal::MissingConstantRegister {
                stage,
                register: binding.register,
                source: binding.source,
            });
        }
    }
    Ok(())
}

fn pack_code_on_seed(
    expected_port: PortId,
    vertex_constants: &[ConstantBinding],
    pixel_constants: &[ConstantBinding],
    seed: &PackedLocalBanks,
    pass: ExecutablePassView<'_>,
) -> Result<PassConstantBuffers, ConstantPackRefusal> {
    if pass.port != expected_port || !expected_port.matches_shader_pair(pass.shader_pair) {
        return Err(ConstantPackRefusal::ExecutablePairMismatch);
    }
    let mut vertex = seed.vertex.clone();
    let mut pixel = seed.pixel.clone();
    let mut written_vertex = seed.written_vertex.clone();
    let mut written_pixel = seed.written_pixel.clone();
    for lane in pass.code_constants {
        let start = usize::from(lane.first_row);
        let end = start.saturating_add(usize::from(lane.row_count));
        write_rows(
            lane.stage,
            lane.destination,
            &lane.rows[start..end],
            &mut vertex,
            &mut pixel,
            &mut written_vertex,
            &mut written_pixel,
        )?;
    }
    require_abi_rows(
        RuntimeShaderStage::Vertex,
        vertex_constants,
        &written_vertex,
    )?;
    require_abi_rows(RuntimeShaderStage::Pixel, pixel_constants, &written_pixel)?;
    Ok(PassConstantBuffers { vertex, pixel })
}

pub fn overlay_packed_code_on_banks(
    vertex: &mut [[u32; 4]],
    pixel: &mut [[u32; 4]],
    lanes: &[PackedCodeConstantLane],
) -> Result<u32, ConstantPackRefusal> {
    let mut written = 0u32;
    for lane in lanes {
        let start = usize::from(lane.first_row);
        let end = start.saturating_add(usize::from(lane.row_count));
        overlay_write_rows(
            lane.stage,
            lane.destination,
            &lane.rows[start..end],
            vertex,
            pixel,
        )?;
        written = written.saturating_add(u32::from(lane.row_count));
    }
    Ok(written)
}

fn overlay_write_rows(
    stage: RuntimeShaderStage,
    destination: u16,
    rows: &[[u32; 4]],
    vertex: &mut [[u32; 4]],
    pixel: &mut [[u32; 4]],
) -> Result<(), ConstantPackRefusal> {
    let bank = match stage {
        RuntimeShaderStage::Vertex => &mut *vertex,
        RuntimeShaderStage::Pixel => &mut *pixel,
    };
    for (row, words) in rows.iter().enumerate() {
        let register = destination
            .checked_add(u16::try_from(row).expect("row index fits u16"))
            .ok_or(ConstantPackRefusal::DestinationPastConstantBlock {
                stage,
                destination,
                block_len: bank.len(),
            })?;
        let index = usize::from(register);
        if index >= bank.len() {
            return Err(ConstantPackRefusal::DestinationPastConstantBlock {
                stage,
                destination: register,
                block_len: bank.len(),
            });
        }
        bank[index] = *words;
    }
    Ok(())
}

fn write_rows(
    stage: RuntimeShaderStage,
    destination: u16,
    rows: &[[u32; 4]],
    vertex: &mut [[u32; 4]],
    pixel: &mut [[u32; 4]],
    written_vertex: &mut [bool],
    written_pixel: &mut [bool],
) -> Result<(), ConstantPackRefusal> {
    let (bank, written) = match stage {
        RuntimeShaderStage::Vertex => (&mut *vertex, &mut *written_vertex),
        RuntimeShaderStage::Pixel => (&mut *pixel, &mut *written_pixel),
    };
    for (row, words) in rows.iter().enumerate() {
        let register = destination
            .checked_add(u16::try_from(row).expect("row index fits u16"))
            .ok_or(ConstantPackRefusal::DestinationPastConstantBlock {
                stage,
                destination,
                block_len: bank.len(),
            })?;
        let index = usize::from(register);
        if index >= bank.len() {
            return Err(ConstantPackRefusal::DestinationPastConstantBlock {
                stage,
                destination: register,
                block_len: bank.len(),
            });
        }
        bank[index] = *words;
        written[index] = true;
    }
    Ok(())
}

fn require_abi_rows(
    stage: RuntimeShaderStage,
    bindings: &[ConstantBinding],
    written: &[bool],
) -> Result<(), ConstantPackRefusal> {
    for binding in bindings {
        if binding.source == ConstantSource::ProgramDefined {
            continue;
        }
        let index = usize::from(binding.register);
        if index >= written.len() {
            continue;
        }
        if !written[index] {
            return Err(ConstantPackRefusal::MissingConstantRegister {
                stage,
                register: binding.register,
                source: binding.source,
            });
        }
    }
    Ok(())
}

pub fn split_bind_layout(
    layout: &WgpuPassLayout,
) -> (
    Vec<super::WgpuBindLayoutEntry>,
    Vec<super::WgpuBindLayoutEntry>,
) {
    let mut constants = Vec::new();
    let mut textures = Vec::new();
    for entry in &layout.bind_entries {
        match entry.group {
            0 => constants.push(*entry),
            1 => textures.push(*entry),
            _ => {}
        }
    }
    (constants, textures)
}
