use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use frame::LaunchIdentity;
use net::{AuthorityClock, AuthorityWorld, PresentedSnapshot};

use crate::ConsoleCommand;

use super::echo::ConsoleEcho;
use super::hitvol::hitvol_report;

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct SceneLifetimeDump<'w, 's> {
    scope: Option<Res<'w, frame::ScopeRegistry>>,
    remote_bodies: Query<'w, 's, &'static render_anim::anim::remote_body::RemoteBodyState>,
    dynents: Query<
        'w,
        's,
        (
            &'static render_frontend::prepare::scene::world::WorldDynEntInstance,
            &'static Transform,
            &'static Visibility,
        ),
    >,
    dyn_plan: Option<Res<'w, render_anim::DynEntDrawPlan>>,
    script_models: Query<
        'w,
        's,
        (
            &'static render_frontend::prepare::scene::world::WorldScriptModelInstance,
            &'static Transform,
            &'static Visibility,
        ),
    >,
    script_plan: Option<Res<'w, render_anim::ScriptModelDrawPlan>>,
    smodel_lighting:
        Option<Res<'w, render_frontend::prepare::scene::smodel_lighting::WorldSmodelLighting>>,
    entities: Query<'w, 's, &'static net::CEntity>,
    fpv_gap: Option<Res<'w, render_anim::occupancy::fpv_present::FpvStatusGap>>,
    fpv_plan: Option<Res<'w, render_anim::draw::FpvDrawPlan>>,
}

impl SceneLifetimeDump<'_, '_> {
    fn append_to(
        &self,
        audio: &mut String,
        scene: &Option<Res<render_frontend::prepare::scene::world::WorldScene>>,
        products: &Option<
            Res<render_frontend::assemble::drawsurf::frame_products::RenderFrameProducts>,
        >,
    ) {
        audio.push_str("\n[client_lifetimes]\n");
        audio.push_str(&format!(
            "centities_live = {}\n",
            self.entities.iter().count()
        ));
        if let Some(owners) = self.scope.as_ref() {
            audio.push_str(&format!(
                "retirement_serial = {}\nretired_resources = {}\n",
                owners.retirement_serial, owners.retired_resources
            ));
            let mut live_resources = 0;
            for (name, scope, kind, live) in owners.lifetimes() {
                if matches!(
                    kind,
                    frame::scope::ScopeKind::Resource | frame::scope::ScopeKind::Staged
                ) {
                    live_resources += usize::from(live);
                }
                audio.push_str(&format!(
                    "resource = {name:?} scope={scope:?} kind={kind:?} live={}\n",
                    usize::from(live)
                ));
            }
            audio.push_str(&format!("scope_resources_live = {live_resources}\n"));
        }
        let tree_bindings = self
            .remote_bodies
            .iter()
            .filter(|state| state.has_tree())
            .count();
        let tree_storage = self.remote_bodies.iter().count()
            * std::mem::size_of::<Option<render_anim::anim::remote_body::PersistentRemoteTree>>();
        audio.push_str(&format!(
            "remote_tree_bindings = {tree_bindings}\nremote_tree_storage_bytes = {tree_storage}\n"
        ));
        audio.push_str("\n[dyn_ent_instances]\n");
        let mut dynents: Vec<_> = self.dynents.iter().collect();
        dynents.sort_by_key(|(inst, _, _)| inst.index);
        for (inst, pose, visibility) in dynents {
            audio.push_str(&format!("id={} model={:?} world={} origin={:?} lighting_origin={:?} visible={visibility:?} dead={}\n",
                inst.index,inst.current_model.0,sim::world_graph::world_at(pose.translation.to_array()),pose.translation,inst.lighting_origin,inst.dead));
        }
        if let Some(plan) = self.dyn_plan.as_ref() {
            audio.push_str(&format!(
                "dyn_ent_draws={} owners={:?}\n",
                plan.draws().len(),
                plan.owners()
            ));
            for draw in plan.draws() {
                audio.push_str(&format!("dyn_ent_draw owner={} world={} origin={:?} surface={} material={} refusal={:?}\n",
                    draw.object_id,sim::world_graph::world_at(draw.world_from_local.w_axis.truncate().to_array()),draw.world_from_local.w_axis,draw.surface,draw.material,draw.colour_refusal));
            }
        }
        audio.push_str("\n[viewmodel]\n");
        audio.push_str("\n[script_model_instances]\n");
        for (owner, pose, visibility) in &self.script_models {
            audio.push_str(&format!(
                "id={} entity={:?} model={:?} origin={:?} visible={visibility:?}\n",
                owner.id.source_ordinal(),
                owner.gentity_number,
                owner.current_model.0,
                pose.translation
            ));
        }
        if let Some(plan) = self.script_plan.as_ref() {
            audio.push_str(&format!(
                "script_model_draws={} owners={:?}\n",
                plan.draws().len(),
                plan.owners()
            ));
        }
        if let Some(scene) = scene.as_ref() {
            let mut drawn = std::collections::BTreeMap::<u32, usize>::new();
            if let Some(products) = products.as_ref() {
                for item in &products
                    .product(render_frame::FrameProductKind::Colour)
                    .ordered_draws
                {
                    if let render_frame::RetainedDrawKind::Smodel {
                        visibility_slot, ..
                    } = item.kind
                    {
                        *drawn.entry(visibility_slot).or_default() += 1;
                    }
                }
            }
            audio.push_str("\n[smodel_instances]\n");
            for (slot, instance) in scene.static_model_instances.iter().enumerate() {
                let Some(instance) = instance else {
                    continue;
                };
                let name = scene
                    .smodel_mesh_names
                    .get(instance.mesh)
                    .map(String::as_str);
                let visible = scene
                    .cull
                    .as_ref()
                    .and_then(|cull| cull.smodel_vis.get(slot))
                    .copied();
                let lighting = self.smodel_lighting.as_ref();
                audio.push_str(&format!("slot={slot} mesh={} model={name:?} origin={:?} visible={visible:?} draws={} handle={:?} tile={} packed={:?}\n",
                    instance.mesh, instance.origin, drawn.get(&(slot as u32)).copied().unwrap_or(0),
                    lighting.and_then(|l| l.handles.get(slot)),
                    lighting.and_then(|l| l.tiles.get(slot)).is_some_and(Option::is_some),
                    lighting.and_then(|l| l.packed_lighting.get(slot))));
            }
        }
        if let Some(status) = self.fpv_gap.as_ref() {
            audio.push_str(&format!("fpv_state = {:?}\n", status.0));
        }
        if let Some(plan) = self.fpv_plan.as_ref() {
            audio.push_str(&format!(
                "fpv_visible = {}\nfpv_draws = {}\nfpv_vertices = {}\nfpv_transform = {:?}\n",
                plan.visible,
                plan.draws().len(),
                plan.decoded_n(),
                plan.world_from_local
            ));
        }
    }
}

pub(crate) fn route_state_dump_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut echo: ConsoleEcho,
    dump: StateDumpWriter,
    lifetimes: SceneLifetimeDump,
    portal: (
        Option<Res<assets::MergedMap>>,
        Option<Res<render_frontend::assemble::drawsurf::frame_products::MaterialFrameInputs>>,
        Option<Res<render_frontend::prepare::scene::world::WorldScene>>,
        Option<Res<render::diag::render_frame_diag::RenderFrameDiag>>,
        Option<Res<bevy::render::renderer::RenderDevice>>,
        Option<Res<render_frontend::assemble::drawsurf::frame_products::RenderFrameProducts>>,
    ),
    (audio_ready, decisions, gaps, clips, runtime): (
        Option<Res<audio::AudioReady>>,
        Option<Res<audio::StartDecisions>>,
        Option<Res<audio::MissingAliasGaps>>,
        Option<Res<audio::ClipStore>>,
        Option<Res<audio::AudioRuntime>>,
    ),
) {
    for cmd in events.read() {
        if cmd.name != "dump" {
            continue;
        }
        let name = match parse_state_dump_name(&cmd.args) {
            Ok(name) => name,
            Err(error) => {
                echo.write(format!("dump: {error}"));
                continue;
            }
        };
        let mut audio = audio_dump_section(
            audio_ready.as_deref(),
            decisions.as_deref(),
            gaps.as_deref(),
            clips.as_deref(),
            runtime.as_deref(),
        );
        audio.push_str(&portal_dump_section(
            portal.0.as_deref(),
            portal.1.as_deref(),
            portal.2.as_deref(),
            portal.3.as_deref(),
            portal.4.as_deref(),
            portal.5.as_deref(),
        ));
        lifetimes.append_to(&mut audio, &portal.2, &portal.5);
        match dump.write(&name, Some(&audio)) {
            Ok(path) => echo.write(format!("dump: wrote {}", path.display())),
            Err(error) => echo.write(format!("dump: {error}")),
        }
    }
}

fn parse_state_dump_name(args: &[String]) -> Result<String, String> {
    let raw = match args {
        [] => "snapshot",
        [name] => name.strip_suffix(".txt").unwrap_or(name),
        _ => return Err("usage: dump [name]".into()),
    };
    if raw.is_empty()
        || raw.len() > 96
        || !raw
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || matches!(raw, "." | "..")
    {
        return Err("name must be 1-96 ASCII letters, digits, '.', '-' or '_'".into());
    }
    Ok(raw.to_owned())
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct StateDumpWriter<'w> {
    identity: Option<Res<'w, LaunchIdentity>>,
    authority_clock: Option<Res<'w, AuthorityClock>>,
    authority: Option<Res<'w, AuthorityWorld>>,
    presented: Option<Res<'w, PresentedSnapshot>>,
}

impl StateDumpWriter<'_> {
    pub(super) fn write(&self, name: &str, audio: Option<&str>) -> Result<PathBuf, String> {
        let identity = self
            .identity
            .as_deref()
            .ok_or("launch identity missing (artifacts path unknown)")?;
        let captured_unix_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before UNIX epoch: {error}"))?
            .as_nanos();
        let body = state_dump_body(
            identity,
            captured_unix_ns,
            self.authority_clock.as_deref(),
            self.authority.as_deref(),
            self.presented.as_deref(),
            audio,
        );
        persist_state_dump(&identity.artifacts, name, captured_unix_ns, &body)
    }
}

pub(super) fn audio_dump_section(
    ready: Option<&audio::AudioReady>,
    decisions: Option<&audio::StartDecisions>,
    gaps: Option<&audio::MissingAliasGaps>,
    clips: Option<&audio::ClipStore>,
    runtime: Option<&audio::AudioRuntime>,
) -> String {
    let report = ready.map(|ready| ready.0);
    let ready = report.is_some_and(|report| report.is_ready());
    let state = report.map_or("Missing", |report| match report.state {
        frame::ReadinessState::Pending => "Pending",
        frame::ReadinessState::Ready => "Ready",
        frame::ReadinessState::Silent => "Silent",
        frame::ReadinessState::Failed => "Failed",
    });
    let late = clips.map(audio::ClipStore::late_prepares).unwrap_or(0);
    let mut out =
        format!("[audio]\nAudioReady = {ready}\naudio_state = {state}\nlate_prepares = {late}\n");
    if let Some(runtime) = runtime {
        out.push_str(&format!("transport = {:?}\n", runtime.diagnostics()));
    }
    match gaps {
        Some(gaps) if !gaps.is_empty() => {
            out.push_str("missing_aliases =\n");
            for alias in &gaps.aliases {
                out.push_str("  ");
                out.push_str(alias);
                out.push('\n');
            }
        }
        _ => out.push_str("missing_aliases = []\n"),
    }
    out.push_str("starts =\n");
    match decisions {
        Some(decisions) => {
            let mut n = 0usize;
            for line in decisions.lines() {
                n += 1;
                out.push_str("  ");
                out.push_str(&line);
                out.push('\n');
            }
            if n == 0 {
                out.push_str("  (none)\n");
            }
        }
        None => out.push_str("  Unavailable { reason: \"StartDecisions resource absent\" }\n"),
    }
    out
}

pub(super) fn state_dump_body(
    identity: &LaunchIdentity,
    captured_unix_ns: u128,
    authority_clock: Option<&AuthorityClock>,
    authority: Option<&AuthorityWorld>,
    presented: Option<&PresentedSnapshot>,
    audio: Option<&str>,
) -> String {
    let authority_snapshot = match (authority_clock, authority) {
        (Some(clock), Some(world)) => format!("{:#?}", world.0.snapshot(sim::Tick(clock.tick))),
        (None, Some(_)) => "Unavailable { reason: \"AuthorityClock resource absent\" }".to_owned(),
        (Some(_), None) => "Unavailable { reason: \"AuthorityWorld resource absent\" }".to_owned(),
        (None, None) => {
            "Unavailable { reason: \"AuthorityClock and AuthorityWorld resources absent\" }"
                .to_owned()
        }
    };
    let presented_snapshot = presented
        .map(|snapshot| format!("{snapshot:#?}"))
        .unwrap_or_else(|| {
            "Unavailable { reason: \"PresentedSnapshot resource absent\" }".to_owned()
        });
    let audio =
        audio.unwrap_or("[audio]\nUnavailable { reason: \"not captured with this dump\" }\n");
    let hitvol = match authority {
        Some(world) => {
            let mut out = String::from("[hitvol]\n");
            for line in hitvol_report(&world.0) {
                out.push_str(&line);
                out.push('\n');
            }
            out.push_str("rows =\n");
            for row in world.0.hitvol_dump() {
                out.push_str(&format!("  {row:?}\n"));
            }
            out
        }
        None => "[hitvol]\nUnavailable { reason: \"AuthorityWorld resource absent\" }\n".to_owned(),
    };
    let lifetimes = authority
        .map(|world| world.0.entity_lifetime_summary())
        .unwrap_or_else(|| "authority_world_present = 0\noccupied_slots = 0\nevent_holds = 0\nprojectile_payloads = 0\nitem_payloads = 0\nmover_payloads = 0\nscript_objects = 0\nscript_defined_entities = 0\nentity_consistency = Ok(())\n".into());
    format!(
        "format = \"iw4l-state-dump-1\"\n\
         captured_unix_ns = {captured_unix_ns}\n\
         role = {:?}\n\
         zone = {:?}\n\
         authority_clock = {authority_clock:#?}\n\
         \n[authority_snapshot]\n{authority_snapshot}\n\
         \n[presented_snapshot]\n{presented_snapshot}\n\
         \n[entity_lifetimes]\n{lifetimes}\n{hitvol}\n{audio}",
        identity.role_label, identity.zone,
    )
}

fn persist_state_dump(
    artifacts: &Path,
    name: &str,
    captured_unix_ns: u128,
    body: &str,
) -> Result<PathBuf, String> {
    let directory = artifacts.join("dumps");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    let file_name = format!("{captured_unix_ns}-{name}.txt");
    let path = directory.join(&file_name);
    persist_bytes_atomic(&path, body)?;
    Ok(path)
}

pub(super) fn persist_bytes_atomic(path: &Path, body: &str) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", path.display()))?;
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("{} has no file name", path.display()))?
        .to_string_lossy();
    let temporary = directory.join(format!(".{file_name}.tmp"));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("create {}: {error}", temporary.display()))?;
        file.write_all(body.as_bytes())
            .map_err(|error| format!("write {}: {error}", temporary.display()))?;
        file.flush()
            .map_err(|error| format!("flush {}: {error}", temporary.display()))?;
        drop(file);
        std::fs::hard_link(&temporary, path).map_err(|error| {
            format!(
                "link {} to {}: {error}",
                temporary.display(),
                path.display()
            )
        })?;
        std::fs::remove_file(&temporary)
            .map_err(|error| format!("remove {}: {error}", temporary.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// A dump or clip must explain the active mechanism, not only name a JSON file
/// elsewhere on the machine. Include the frozen recipe and the unfolded views
/// actually chosen by the renderer at this camera pose.
pub(super) fn portal_dump_section(
    merged: Option<&assets::MergedMap>,
    frame: Option<&render_frontend::assemble::drawsurf::frame_products::MaterialFrameInputs>,
    scene: Option<&render_frontend::prepare::scene::world::WorldScene>,
    timings: Option<&render::diag::render_frame_diag::RenderFrameDiag>,
    device: Option<&bevy::render::renderer::RenderDevice>,
    products: Option<&render_frontend::assemble::drawsurf::frame_products::RenderFrameProducts>,
) -> String {
    let mut out = merged.map(|merged| format!(
        "\n[portal_layout]\nname={:?}\nrecording_zone={:?}\nsnapshot={:?}\nrecipe_json={}\n",
        merged.name,
        merged.recording_name(),
        merged.path,
        merged.recipe
    )).unwrap_or(String::new());
    if let Some(frame) = frame {
        out.push_str(&format!(
            "\n[portal_render]\neye={:?}\nmask=stencil\nview={:#?}\nworld_fog_colors={:?}\n",
            frame.view_origin, frame.portal, frame.world_fog_colors
        ));
        if let Some(portal) = frame.portal.as_ref()
            && let Some(products) = products
        {
            out.push_str(&format!(
                "\n[portal_entity_slices]\nbounds={} pairs={}\n",
                portal.entity_bounds.len(),
                portal.pairs.len()
            ));
            let mut seen = std::collections::BTreeSet::new();
            for item in &products
                .product(render_frame::FrameProductKind::Colour)
                .ordered_draws
            {
                if let render_frame::RetainedDrawKind::XModel {
                    scene_entnum: Some(entity),
                    object_id,
                    ..
                } = item.kind
                    && seen.insert(entity)
                {
                    let copies = portal.entity_instances(&item.kind);
                    if copies.len() > 1 {
                        out.push_str(&format!(
                            "entity={entity} object={object_id} instances={copies:?}\n"
                        ));
                    }
                }
            }
        }
        if let Some(portal) = frame.portal.as_ref()
            && let Some(plan) = portal.plan.as_ref()
        {
            out.push_str(&format!("blocked_exits={:?}\n", portal.blocked_exits));
            for aperture in &plan.apertures {
                let rect = aperture.projection.rect;
                let visible = portal
                    .visibility
                    .get(aperture.child)
                    .and_then(Option::as_ref);
                let surfaces = visible.map(|v| v.surfaces.iter().filter(|&&b| b != 0).count());
                let smodels = visible.map(|v| v.smodels.iter().filter(|&&b| b != 0).count());
                let bmodels = visible.map(|v| v.bmodels.len());
                out.push_str(&format!(
                    "view_decision view={} tag={} world={:?} parent={} rect={rect:?} area={} selected={} fallback={:?} eye={:?} surfaces={surfaces:?} smodels={smodels:?} bmodels={bmodels:?} exit_clip_mode={:?} exit_plane={:?} fog_color={:?} sun_shadow={:?} sun_receiver_present={:?}\n",
                    aperture.child, aperture.child + 1, portal.worlds.get(aperture.child), aperture.parent,
                    u64::from(rect[2]) * u64::from(rect[3]),
                    plan.selected.get(aperture.child).copied().unwrap_or(false),
                    aperture.fallback, portal.eye_in(aperture.child, frame.view_origin.to_array()),
                    frame.world_cameras.get(aperture.child).and_then(Option::as_ref).and_then(|c| c.exit_plane_clip).and_then(|_| device.map(render::exit_clip_mode)),
                    portal.exit_plane_in(aperture.child, frame.view_origin.to_array()),
                    portal.worlds.get(aperture.child).and_then(|world| frame.world_fog_colors.get(*world)),
                    frame.world_cameras.get(aperture.child).and_then(Option::as_ref).map(|c| c.sun_shadow),
                    frame.world_cameras.get(aperture.child).and_then(Option::as_ref).map(|c| c.code_sources.constant_arc(0x5a).is_some()),
                ));
            }
        }
    }
    if let Some(products) = products {
        // The producer moves the caster plan into published GPU products.
        // Read the exact published list, not the now-empty staging vector.
        let spot = products.product(render_frame::FrameProductKind::SpotShadow);
        let sliced = spot
            .ordered_draws
            .iter()
            .filter(|item| {
                matches!(
                    item.kind,
                    render_frame::RetainedDrawKind::XModel {
                        portal_slice: Some(_),
                        ..
                    }
                )
            })
            .count();
        out.push_str(&format!("\n[spot_entity_slices]\nsource=published_frame_products status={:?} slots={} rows={} sliced={}\n",
            spot.status, spot.spot_slots.len(), spot.ordered_draws.len(), sliced));
        for slot in &spot.spot_slots {
            out.push_str(&format!(
                "spot_slot index={} light={} view={:?}\n",
                slot.emitted.slot_index, slot.emitted.light_index, slot.emitted.view_parms
            ));
        }
        let sun = products.product(render_frame::FrameProductKind::SunShadow);
        let surfaces: Vec<_> = sun
            .ordered_draws
            .iter()
            .filter_map(|item| match item.kind {
                render_frame::RetainedDrawKind::World { surf, .. } => Some(surf),
                _ => None,
            })
            .collect();
        let portal = frame.and_then(|frame| frame.portal.as_ref());
        let mut worlds = std::collections::BTreeMap::<usize, [usize; 3]>::new();
        let mut foreign = 0;
        let mut unknown = 0;
        for item in &sun.ordered_draws {
            let (world, kind) = match item.kind {
                render_frame::RetainedDrawKind::World { surf, .. } => (
                    portal.map_or(0, |p| {
                        usize::from(p.surface_tag(u32::from(surf))).saturating_sub(1)
                    }),
                    0,
                ),
                render_frame::RetainedDrawKind::Smodel {
                    owner_world_from_local,
                    ..
                } => (
                    portal.map_or(0, |p| {
                        usize::from(
                            p.point_tag(owner_world_from_local.w_axis.truncate().to_array()),
                        )
                        .saturating_sub(1)
                    }),
                    1,
                ),
                render_frame::RetainedDrawKind::XModel {
                    world_from_local, ..
                } => (
                    portal.map_or(0, |p| {
                        usize::from(p.point_tag(world_from_local.w_axis.truncate().to_array()))
                            .saturating_sub(1)
                    }),
                    2,
                ),
                _ => {
                    unknown += 1;
                    continue;
                }
            };
            worlds.entry(world).or_default()[kind] += 1;
            foreign += usize::from(world != products.camera_world);
        }
        out.push_str(&format!("\n[root_sun_casters]\nsource=published_frame_products frame_id={} camera_world={} eye={:?} status={:?}\nitems={} near={} foreign={} unknown={} world_surfaces={} surface_min={:?} surface_max={:?}\nworld_counts_bsp_smodel_xmodel={worlds:?}\n",
            products.frame_id, products.camera_world, products.view_origin, sun.status,
            sun.ordered_draws.len(), sun.sun_near_n, foreign, unknown, surfaces.len(), surfaces.iter().min(), surfaces.iter().max()));
        for (row, item) in sun.ordered_draws.iter().enumerate() {
            if let render_frame::RetainedDrawKind::XModel {
                world_from_local,
                portal_slice: Some(plane),
                scene_entnum,
                ..
            } = item.kind
            {
                out.push_str(&format!("sun_entity_slice row={row} near={} entity={scene_entnum:?} origin={:?} plane={plane:?}\n", row < sun.sun_near_n as usize, world_from_local.w_axis.truncate().to_array()));
            }
        }
        if let Some(cull) = scene.and_then(|s| s.cull.as_ref()) {
            let mut ranges: Vec<_> = cull
                .atlas_layers
                .iter()
                .map(|l| {
                    (
                        l.world,
                        l.surface_base,
                        l.surface_base + l.surface_count,
                        l.smodel_base,
                        l.dpvs.smodel_bounds.len(),
                    )
                })
                .collect();
            ranges.push((
                0,
                0,
                cull.atlas_layers
                    .first()
                    .map_or(cull.surface_materials.len(), |l| l.surface_base),
                0,
                cull.dpvs.smodel_bounds.len(),
            ));
            ranges.sort_by_key(|r| r.0);
            for (world, begin, end, smodel_base, smodel_count) in ranges {
                let out_of_world = if world == products.camera_world {
                    surfaces
                        .iter()
                        .filter(|&&surf| !(begin..end).contains(&usize::from(surf)))
                        .count()
                } else {
                    0
                };
                out.push_str(&format!("caster_domain world={world} surfaces={begin}..{end} smodels={smodel_base}..{} outside_active_surface_range={out_of_world}\n",smodel_base+smodel_count));
            }
        }
    }
    if let Some(timings) = timings {
        out.push_str(&format!(
            "\n[portal_timings]\nbatch=latest_delivered\nviews={:#?}\n",
            timings.portal_timings
        ));
    }
    if let Some(scene) = scene {
        out.push_str(&format!(
            "\n[portal_sky]\nworld_count={}\nsky_worlds={:?}\n",
            merged.map_or(1, |merged| merged.worlds.len()),
            scene.sky_worlds
        ));
        out.push_str(&format!("view_visibility={:#?}\n", scene.portal_visibility));
        if let Some(cull) = scene.cull.as_ref() {
            out.push_str("\n[brush_models]\n");
            for (model, surfs) in cull.brush_models.iter().enumerate().skip(1) {
                if surfs.surface_count == 0 {
                    continue;
                }
                let start = usize::from(surfs.start_surf);
                let end = start + usize::from(surfs.surface_count);
                let materials: std::collections::BTreeSet<_> = (start..end)
                    .filter_map(|i| cull.surface_materials.get(i).copied().flatten())
                    .filter_map(|id| {
                        scene
                            .runtime_material_catalog
                            .parts()
                            .materials
                            .get(id.order())
                    })
                    .map(|material| material.name.as_str())
                    .collect();
                let admitted = cull
                    .draws
                    .items
                    .iter()
                    .filter(|item| {
                        item.kind
                            == render_frontend::prepare::scene::world::WorldDrawItemKind::BModel
                    })
                    .map(|item| {
                        (usize::from(item.surf) + usize::from(item.run))
                            .min(end)
                            .saturating_sub(usize::from(item.surf).max(start))
                    })
                    .sum::<usize>();
                out.push_str(&format!("model={model} surfaces={start}..{end} admitted={admitted} materials={materials:?} pose={:?}\n",
                    cull.bmodel_world_from_local.get(model)));
            }
            let count = if cull.cell_vis_world == 0 {
                cull.dpvs.cell_count
            } else {
                cull.atlas_layers
                    .iter()
                    .find(|layer| layer.world == cull.cell_vis_world)
                    .map_or(0, |layer| layer.dpvs.cell_count)
            };
            out.push_str(&format!(
                "root_cell_domain world={} count={} words={:?}\n",
                cull.cell_vis_world, count, cull.cell_vis
            ));
            out.push_str("\n[atlas_brush_owners]\n");
            for layer in &cull.atlas_layers {
                for &(model, bounds) in &layer.bmodel_bounds {
                    let Some(surfs) = cull.brush_models.get(usize::from(model)) else {
                        continue;
                    };
                    if surfs.surface_count == 0 {
                        continue;
                    }
                    let start = usize::from(surfs.start_surf);
                    let end = start + usize::from(surfs.surface_count);
                    let admitted = cull
                        .draws
                        .items
                        .iter()
                        .filter(|item| {
                            item.kind
                                == render_frontend::prepare::scene::world::WorldDrawItemKind::BModel
                        })
                        .map(|item| {
                            let first = usize::from(item.surf).max(start);
                            let last = (usize::from(item.surf) + usize::from(item.run)).min(end);
                            last.saturating_sub(first)
                        })
                        .sum::<usize>();
                    out.push_str(&format!("world={} model={model} surfaces={start}..{end} admitted={admitted} bounds={bounds:?} pose={:?}\n",
                        layer.world, cull.bmodel_world_from_local.get(usize::from(model))));
                }
            }
        }
        for (model, world) in scene.sky_models.iter().zip(&scene.sky_worlds) {
            out.push_str(&format!("world={world} model={:?}\n", model.name));
        }
    }
    out
}
