use bevy::prelude::*;
use render_frame::water_sheeting::WaterSheetingFrame;

#[derive(Resource, Default)]
pub struct WaterSheetingView {
    pub current: Option<WaterSheetingFrame>,
    owner: Option<sim::ClientId>,
    applied: Option<sim::ScriptWaterSheeting>,
    distortion: [f32; 4],
}

struct WaterSettings {
    allowed: bool,
    forced: bool,
    distortion: [f32; 4],
}

impl WaterSettings {
    fn read(script: sim::ScriptDvars<'_>, local: &frame::UiMenuDvars) -> Self {
        let value = |name| script.string(name).or_else(|| local.get(name));
        let number = |name, default: f32| {
            value(name)
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .unwrap_or(default)
        };
        let scale = value("cg_waterSheeting_distortionScaleFactor")
            .and_then(|text| {
                let values = text
                    .split_whitespace()
                    .map(str::parse::<f32>)
                    .collect::<Result<Vec<_>, _>>()
                    .ok()?;
                let row: [f32; 4] = values.try_into().ok()?;
                row.iter()
                    .all(|v| v.is_finite())
                    .then_some(row.map(|v| v.clamp(0.0, 1.0)))
            })
            .unwrap_or([0.021961, 1.0, 0.0, 0.0]);
        Self {
            allowed: number("cg_watersheeting", 1.0) != 0.0,
            forced: number("cg_waterSheeting_enable", 0.0) != 0.0,
            distortion: [
                scale[0],
                0.3,
                if scale[1] > 0.0 {
                    scale[1].recip()
                } else {
                    0.0
                },
                number("cg_waterSheeting_magnitude", 0.0655388).clamp(0.0, 1.0),
            ],
        }
    }
}

pub fn register(app: &mut App) {
    app.init_resource::<WaterSheetingView>().add_systems(
        Update,
        update_water_sheeting
            .after(crate::prepare::scene::view_parms::stamp_prepared_scene_view)
            .in_set(net::ClientSet::Present),
    );
}

fn update_water_sheeting(
    mut water: ResMut<WaterSheetingView>,
    view: Res<crate::prepare::scene::view_parms::PreparedSceneView>,
    presentation: net::LocalPresentation,
    clock: Option<Res<net::FrameClock>>,
    local: Res<frame::UiMenuDvars>,
) {
    let Some(clock) = clock else {
        *water = WaterSheetingView::default();
        return;
    };
    let Some(snapshot) = presentation.snapshot().filter(|_| view.ready) else {
        *water = WaterSheetingView::default();
        return;
    };
    let client = presentation.client_id();
    let Some(meta) = snapshot.meta.for_client(client) else {
        *water = WaterSheetingView::default();
        return;
    };
    if water.owner != Some(client) {
        *water = WaterSheetingView {
            owner: Some(client),
            ..Default::default()
        };
    }
    let settings = WaterSettings::read(snapshot.meta.script_dvars(client), &local);
    let requested = meta.view_effects.water_sheeting;
    if requested != water.applied {
        water.applied = requested;
        water.distortion = settings.distortion;
    }
    water.current = if !settings.allowed {
        None
    } else if settings.forced {
        Some(WaterSheetingFrame {
            distortion: settings.distortion,
            fade: 0.0,
        })
    } else {
        requested
            .and_then(|effect| effect.sample(clock.time()))
            .map(|fade| WaterSheetingFrame {
                distortion: water.distortion,
                fade,
            })
    };
}
