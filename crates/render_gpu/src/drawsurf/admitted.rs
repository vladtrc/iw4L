use std::sync::Arc;

use d3d9_sm3::PassWgsl;
use render_frame::WgpuPassLayout;
use render_material::{
    ExecutablePassView, PackedCodeConstantLane, PassProgramAbi, PortId, RuntimeShaderStage,
};

use super::gpu_prepare::{
    ConstantPackRefusal, PassConstantBuffers, overlay_lane_bytes, pack_code_on_seed,
    require_abi_rows_with_lanes,
};

#[derive(Clone, Debug)]
pub struct AdmittedExactPort {
    pub id: PortId,
    pub abi: PassProgramAbi,
    pub module: Arc<PassWgsl>,
    pub layout: WgpuPassLayout,
}

impl AdmittedExactPort {
    pub fn id(&self) -> PortId {
        self.id
    }

    pub fn abi(&self) -> &PassProgramAbi {
        &self.abi
    }

    pub fn module(&self) -> &PassWgsl {
        &self.module
    }

    pub fn shared_module(&self) -> Arc<PassWgsl> {
        Arc::clone(&self.module)
    }

    pub fn wgpu_layout(&self) -> &WgpuPassLayout {
        &self.layout
    }

    pub fn accepts_pass(&self, pass: ExecutablePassView<'_>) -> bool {
        pass.port == self.id && pass.port.matches_shader_pair(pass.shader_pair)
    }

    pub fn pack_hit(
        &self,
        pass: ExecutablePassView<'_>,
    ) -> Result<PassConstantBuffers, ConstantPackRefusal> {
        let Some(seed) = pass.local_banks.map(Arc::as_ref) else {
            return Err(ConstantPackRefusal::MissingLocalBanks);
        };
        if seed.vertex.len() == self.module.vertex_constant_len
            && seed.pixel.len() == self.module.pixel_constant_len
            && seed.written_vertex.len() == seed.vertex.len()
            && seed.written_pixel.len() == seed.pixel.len()
        {
            return pack_code_on_seed(
                self.id,
                &self.abi.vertex_constants,
                &self.abi.pixel_constants,
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
        if seed.vertex.len() != self.module.vertex_constant_len
            || seed.pixel.len() != self.module.pixel_constant_len
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
            &self.abi.vertex_constants,
            &seed.written_vertex,
            pass.code_constants,
        )?;
        require_abi_rows_with_lanes(
            RuntimeShaderStage::Pixel,
            &self.abi.pixel_constants,
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
