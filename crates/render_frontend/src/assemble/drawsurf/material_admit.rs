use std::sync::Arc;

use bevy::asset::{AssetId, Assets, Handle};
use bevy::shader::Shader;

use super::sm3_wgsl::ValidatedPassWgsl;
use super::{MaterialProgramCompile, RuntimeProgramRegistry};

fn shader_from_generated_wgsl(source: String, path: String) -> Shader {
    if source.contains('#') {
        return Shader::from_wgsl(source, path);
    }
    Shader {
        import_path: bevy::shader::ShaderImport::AssetPath(path.clone()),
        path,
        source: bevy::shader::Source::Wgsl(source.into()),
        imports: Vec::new(),
        additional_imports: Vec::new(),
        shader_defs: Vec::new(),
        file_dependencies: Vec::new(),
        validate_shader: bevy::shader::ValidateShader::Disabled,
    }
}

enum PendingShader {
    Generated {
        module: Arc<ValidatedPassWgsl>,
        path: String,
    },
}

impl PendingShader {
    fn build(self) -> Shader {
        let Self::Generated { module, path } = self;
        shader_from_generated_wgsl(module.source.clone(), path)
    }
}

#[derive(Default)]
pub struct MaterialProgramAdmit {
    armed: bool,
    registry: RuntimeProgramRegistry,
    exact_shaders: Vec<Handle<Shader>>,
    pending: std::collections::HashMap<AssetId<Shader>, PendingShader>,
    requests: Vec<AssetId<Shader>>,
    stage: Option<assets::LoadStage>,
}

impl MaterialProgramAdmit {
    pub fn armed(&self) -> bool {
        self.armed
    }

    pub fn port_count(&self) -> usize {
        self.registry.ports().len()
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn take_generation(&mut self) -> (RuntimeProgramRegistry, Vec<Handle<Shader>>) {
        (
            std::mem::take(&mut self.registry),
            std::mem::take(&mut self.exact_shaders),
        )
    }

    pub fn take_stage(&mut self) -> Option<assets::LoadStage> {
        self.stage.take()
    }

    pub fn request_if_pending(&mut self, id: AssetId<Shader>) {
        if self.pending.contains_key(&id) {
            self.requests.push(id);
        }
    }

    pub fn supply(&mut self, shaders: &mut Assets<Shader>) {
        for id in std::mem::take(&mut self.requests) {
            if let Some(shader) = self.pending.remove(&id) {
                shaders
                    .insert(id, shader.build())
                    .expect("reserved shader handle is live in this generation");
            }
        }
    }

    pub fn arm(
        &mut self,
        compile: &mut MaterialProgramCompile,
        progress: Option<&assets::LoadProgress>,
    ) {
        if let Some(progress) = progress {
            self.stage = Some(progress.stage("admitting world shaders"));
        }
        let ports = compile.take_ports();
        let world_port_count = ports
            .iter()
            .filter(|port| {
                let vertex_type = port.abi().vertex_type;
                vertex_type != asset_iw4::vertex_decl::PACKED_VERTEX_TYPE
                    && vertex_type != asset_iw4::vertex_decl::POS_TEX_VERTEX_TYPE
                    && !compile.is_postfx(port.id())
            })
            .count();
        let packed_port_count = ports
            .iter()
            .filter(|port| port.abi().vertex_type == asset_iw4::vertex_decl::PACKED_VERTEX_TYPE)
            .count();
        let pos_tex_port_count = ports
            .iter()
            .filter(|port| port.abi().vertex_type == asset_iw4::vertex_decl::POS_TEX_VERTEX_TYPE)
            .count();
        let postfx_port_count = ports
            .iter()
            .filter(|port| compile.is_postfx(port.id()))
            .count();
        self.registry = match RuntimeProgramRegistry::from_ports(ports) {
            Ok(programs) => {
                diag::info!(
                    World,
                    "drawsurf production exact ports: READY count={} world={} packed={} pos_tex={} postfx={} refused_world={} refused_packed={} refused_pos_tex={} refused_postfx={} causes_world={:?} causes_packed={:?} causes_pos_tex={:?} causes_postfx={:?}",
                    programs.ports().len(),
                    world_port_count,
                    packed_port_count,
                    pos_tex_port_count,
                    postfx_port_count,
                    compile.refused_world,
                    compile.refused_packed,
                    compile.refused_pos_tex,
                    compile.refused_postfx,
                    compile.world_causes,
                    compile.packed_causes,
                    compile.pos_tex_causes,
                    compile.postfx_causes,
                );
                programs
            }
            Err(cause) => {
                diag::warn!(
                    World,
                    "drawsurf production exact ports: RED from_ports cause={cause:?} refused_world={} refused_packed={} refused_pos_tex={} refused_postfx={} causes_world={:?} causes_packed={:?} causes_pos_tex={:?} causes_postfx={:?}",
                    compile.refused_world,
                    compile.refused_packed,
                    compile.refused_pos_tex,
                    compile.refused_postfx,
                    compile.world_causes,
                    compile.packed_causes,
                    compile.pos_tex_causes,
                    compile.postfx_causes,
                );
                RuntimeProgramRegistry::default()
            }
        };
        self.exact_shaders.clear();
        self.armed = true;
        if let Some(stage) = &self.stage {
            stage.total(self.registry.ports().len() as u64);
        }
    }

    pub fn reserve_shaders(&mut self, shaders: &Assets<Shader>) {
        let mut by_source = std::collections::HashMap::new();
        for port in self.registry.ports() {
            let handle = by_source
                .entry(port.module().source.as_str())
                .or_insert_with(|| {
                    let handle = shaders.reserve_handle();
                    self.pending.insert(
                        handle.id(),
                        PendingShader::Generated {
                            module: port.shared_module(),
                            path: format!("iw4_exact_colour/{:?}.wgsl", handle.id()),
                        },
                    );
                    handle
                });
            self.exact_shaders.push(handle.clone());
        }
        if let Some(stage) = &self.stage {
            stage.set_done(self.exact_shaders.len() as u64);
        }
        diag::info!(
            World,
            "world spawn shaders: ports={} unique={} (supplied on pipeline request)",
            self.exact_shaders.len(),
            by_source.len()
        );
    }
}
