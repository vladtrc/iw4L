use bevy::prelude::*;
use frame::{LaunchIdentity, LaunchReport};

use crate::class_store::SessionClassStore;
use crate::gap_hud::GapHud;

pub fn publish_gap_hud(
    identity: Option<Res<LaunchIdentity>>,
    report: Option<Res<LaunchReport>>,
    class_store: Res<SessionClassStore>,
    mut hud: ResMut<GapHud>,
) {
    let Some(identity) = identity else {
        return;
    };
    let Some(report) = report else {
        return;
    };

    let mut body = vec![
        format!("role: {}", identity.role_label),
        format!("IW4L_GAMES: {}", identity.games_root.display()),
        format!("artifacts: {}", identity.artifacts.display()),
        format!("zone request: {}", report.zone),
        format!(
            "match: {} (`{}`) score_limit={} time_limit_ms={}",
            sim::host_game_mode_kind().display_name(),
            sim::host_game_mode_kind().token(),
            sim::FFA.score_limit,
            sim::FFA.time_limit_ms
        ),
    ];
    if let Some(slot) = class_store.equipped_slot() {
        body.push(format!("equipped class: {}", slot.name));
    } else {
        body.push("equipped class: (choose-class gate)".into());
    }
    match &report.common_mp {
        Ok(p) => body.push(format!("found common_mp.ff: {}", p.display())),
        Err(e) => body.push(format!("common_mp.ff: {e}")),
    }
    match &report.zone_ff {
        Ok(p) => body.push(format!("found {}.ff: {}", report.zone, p.display())),
        Err(e) => body.push(format!("{}.ff: {e}", report.zone)),
    }
    body.push(String::new());
    body.push("world:".into());
    for row in &report.world_report {
        body.push(format!("- {row}"));
    }
    body.push(String::new());
    body.push("gaps (not faked):".into());
    body.push(
        "- map zone walk stops after ClipMap (often impactfx); common_mp walks weapon/xanim (next: menulist)"
            .into(),
    );
    body.push(
        "- Equip gives primary via sorted weapon catalog + CmdScale scales; bind-pose FPV under eye when gunXModel resolves"
            .into(),
    );
    body.push(
        "- sky cubemap + alpha-test/blend/multiply from techset+stateBits are live; specular/probe/spot still gaps"
            .into(),
    );
    body.push(
        "- DPVS batches use material identity; retail-runtime MaterialInfo.drawSurf keys not yet"
            .into(),
    );
    body.push(
        "- FPV eye-posed hands+gun (bind baseMat + rigid verts); XAnim sample / full blend skin still gap"
            .into(),
    );
    body.push(
        "- smodel LOD0 placements are live through DPVS; authored prop materials/light-grid remain gaps"
            .into(),
    );
    body.push(format!("- {}", report.sim_gap));
    if let Some(metrics) = &report.prediction_metrics {
        body.push(format!("- prediction: {metrics}"));
    }
    body.push(String::new());
    body.push("WASD move, arrows look, escape / close window to quit".into());

    let title = format!("iw4l — {}", identity.zone);
    let report_changed = report.is_changed();
    let identity_changed = identity.is_changed();
    let class_changed = class_store.is_changed();
    if hud.title == title
        && hud.body == body
        && !report_changed
        && !identity_changed
        && !class_changed
    {
        return;
    }
    hud.title = title;
    hud.body = body;
}
