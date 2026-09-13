use crate::{ConsoleCommand, ConsoleLine, ConsoleSettings, ConsoleState};
use bevy::prelude::*;
use render_frontend::assemble::drawsurf::{
    DistortionSettings, FrameProductKind, MaterialGeneration, RenderFrameProducts,
};

pub(crate) fn register(registry: &mut crate::ConsoleRegistry) {
    registry.register(
        crate::CommandSpec::new("r_distortion")
            .usage("r_distortion [0|1] — enable scene distortion (default 1)"),
    );
    registry.register(
        crate::CommandSpec::new("distortion_dump")
            .usage("distortion_dump — current resolve request and detected materials"),
    );
}

pub(crate) fn route(
    mut commands: MessageReader<ConsoleCommand>,
    mut distortion: ResMut<DistortionSettings>,
    products: Res<RenderFrameProducts>,
    runtime: Res<MaterialGeneration>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
) {
    for cmd in commands.read() {
        let msg = match cmd.name.as_str() {
            "r_distortion" => match cmd.args.as_slice() {
                [] => format!(
                    "r_distortion = {} (default 1)",
                    u8::from(distortion.enabled)
                ),
                [value] if value == "0" || value == "1" => {
                    distortion.enabled = value == "1";
                    format!("r_distortion = {}", u8::from(distortion.enabled))
                }
                _ => "usage: r_distortion [0|1]".to_owned(),
            },
            "distortion_dump" if cmd.args.is_empty() => {
                let product = products.product(FrameProductKind::Distortion);
                let mut materials = std::collections::BTreeMap::new();
                for draw in &product.ordered_draws {
                    let name = runtime
                        .catalog
                        .material_for_sorted_ordinal(draw.material_rank)
                        .map(|material| material.name.as_str())
                        .unwrap_or("<missing material>");
                    *materials.entry(name).or_insert(0usize) += 1;
                }
                format!(
                    "distortion: enabled={} frame={} request={:?} detected={} materials={materials:?}",
                    u8::from(distortion.enabled),
                    products.frame_id,
                    product.status,
                    product.ordered_draws.len(),
                )
            }
            "distortion_dump" => "usage: distortion_dump".to_owned(),
            _ => continue,
        };
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, settings.log_capacity);
    }
}
