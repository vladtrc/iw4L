use bevy::prelude::*;
use frame::{
    GameEnded, GameWin, GlassDestroyed, MatchEndingSoon, MatchEndingVerySoon, MatchTornDown,
    PrematchDone, RoundSwitchNotify, RoundWin, SpawnedPlayerNotify,
};

use crate::plugin::NetPlugin;
use crate::role::RuntimeRole;
use crate::schedule::{
    AUTHORITY_TOC, AuthoritySet, CLIENT_TOC, ClientSet, WORKER_CMD_AFTER, WORKER_CMD_END_FENCE,
    WORKER_CMD_RETAIL_NAMES, WORKER_CMD_TOC, WorkerCmdSet, configure_authority_sets,
    configure_client_sets, worker_cmd_name,
};

#[derive(Resource, Default)]
struct Trace(Vec<&'static str>);

fn push(label: &'static str) -> impl FnMut(ResMut<Trace>) + 'static {
    move |mut t: ResMut<Trace>| t.0.push(label)
}

fn label_authority(set: &AuthoritySet) -> &'static str {
    match set {
        AuthoritySet::Advance => "A:Advance",
        AuthoritySet::Ingress => "A:Ingress",
        AuthoritySet::Gather => "A:Gather",
        AuthoritySet::Step => "A:Step",
        AuthoritySet::Snapshot => "A:Snapshot",
        AuthoritySet::Fanout => "A:Fanout",
        AuthoritySet::Bookkeeping => "A:Bookkeeping",
    }
}

fn label_client(set: &ClientSet) -> &'static str {
    match set {
        ClientSet::Load => "C:Load",
        ClientSet::Receive => "C:Receive",
        ClientSet::Reconcile => "C:Reconcile",
        ClientSet::Input => "C:Input",
        ClientSet::Predict => "C:Predict",
        ClientSet::Send => "C:Send",
        ClientSet::Present => "C:Present",
        ClientSet::Ui => "C:Ui",
        ClientSet::Effects => "C:Effects",
        ClientSet::Diag => "C:Diag",
    }
}

fn expected_authority_trace() -> Vec<&'static str> {
    AUTHORITY_TOC.iter().map(label_authority).collect()
}

fn expected_client_trace() -> Vec<&'static str> {
    CLIENT_TOC.iter().map(label_client).collect()
}

fn install_authority_probes(app: &mut App) {
    for set in AUTHORITY_TOC {
        let label = label_authority(set);
        app.add_systems(FixedUpdate, push(label).in_set(set.clone()));
    }
}

fn install_client_probes(app: &mut App) {
    for set in CLIENT_TOC {
        let label = label_client(set);
        app.add_systems(Update, push(label).in_set(set.clone()));
    }
}

fn probe_app() -> App {
    let mut app = App::new();
    app.add_plugins(bevy::time::TimePlugin);
    app.add_message::<MatchTornDown>();
    app.add_message::<MatchEndingSoon>();
    app.add_message::<MatchEndingVerySoon>();
    app.add_message::<GameEnded>();
    app.add_message::<PrematchDone>();
    app.add_message::<GameWin>();
    app.add_message::<RoundWin>();
    app.add_message::<RoundSwitchNotify>();
    app.add_message::<SpawnedPlayerNotify>();
    app.add_message::<GlassDestroyed>();
    app
}

pub fn check() -> Result<(), String> {
    authority_toc_order()?;
    client_toc_order()?;
    toc_lengths()?;
    worker_cmd_graph()?;
    worker_cmd_nested_in_present()?;
    role(RuntimeRole::Listen, true, true)?;
    role(RuntimeRole::Dedicated, true, false)?;
    role(RuntimeRole::Client, false, true)?;
    role(RuntimeRole::Replay, false, true)?;
    Ok(())
}

pub fn authority_toc_order() -> Result<(), String> {
    let mut app = probe_app();
    app.init_resource::<Trace>();
    configure_authority_sets(&mut app);
    install_authority_probes(&mut app);
    app.world_mut().run_schedule(FixedUpdate);
    let trace = app.world().resource::<Trace>().0.clone();
    let expected = expected_authority_trace();
    if trace != expected {
        return Err(format!(
            "authority FixedUpdate order drifted from schedule.rs TOC: {trace:?} != {expected:?}"
        ));
    }
    Ok(())
}

pub fn client_toc_order() -> Result<(), String> {
    let mut app = probe_app();
    app.init_resource::<Trace>();
    configure_client_sets(&mut app);
    install_client_probes(&mut app);
    app.world_mut().run_schedule(Update);
    let trace = app.world().resource::<Trace>().0.clone();
    let expected = expected_client_trace();
    if trace != expected {
        return Err(format!(
            "client Update order drifted from schedule.rs TOC: {trace:?} != {expected:?}"
        ));
    }
    Ok(())
}

pub fn toc_lengths() -> Result<(), String> {
    if AUTHORITY_TOC.len() != 7 {
        return Err(format!(
            "AUTHORITY_TOC len {} (expected 7)",
            AUTHORITY_TOC.len()
        ));
    }
    if CLIENT_TOC.len() != 10 {
        return Err(format!("CLIENT_TOC len {} (expected 10)", CLIENT_TOC.len()));
    }
    Ok(())
}

pub fn worker_cmd_graph() -> Result<(), String> {
    if WORKER_CMD_TOC.len() != 21 {
        return Err(format!(
            "WORKER_CMD_TOC len {} (expected 21)",
            WORKER_CMD_TOC.len()
        ));
    }
    if WORKER_CMD_RETAIL_NAMES.len() != 21 {
        return Err(format!(
            "WORKER_CMD_RETAIL_NAMES len {} (expected 21)",
            WORKER_CMD_RETAIL_NAMES.len()
        ));
    }
    for (index, set) in WORKER_CMD_TOC.iter().enumerate() {
        let got = *set as u8;
        if got != index as u8 {
            return Err(format!(
                "WorkerCmdSet discriminant {got} at TOC[{index}] (type number is the index)"
            ));
        }
        let name = worker_cmd_name(*set);
        let expected = WORKER_CMD_RETAIL_NAMES[index];
        if name != expected {
            return Err(format!(
                "worker_cmd_name({set:?}) = {name:?}, retail table[{index}] = {expected:?}"
            ));
        }
    }
    for &(later, earlier) in WORKER_CMD_AFTER {
        if later == WorkerCmdSet::SkinModel && earlier == WorkerCmdSet::CellStatic {
            return Err(
                "WORKER_CMD_AFTER orders SkinModel after CellStatic — Present chain under new names"
                    .into(),
            );
        }
        if later == WorkerCmdSet::CellStatic && earlier == WorkerCmdSet::SkinModel {
            return Err(
                "WORKER_CMD_AFTER orders CellStatic after SkinModel — Present chain under new names"
                    .into(),
            );
        }
    }
    let fence = [
        WorkerCmdSet::GlassVerts,
        WorkerCmdSet::MarkVerts,
        WorkerCmdSet::FxVerts,
        WorkerCmdSet::SmodelCache,
        WorkerCmdSet::SkinModel,
    ];
    if WORKER_CMD_END_FENCE != fence.as_slice() {
        return Err(format!(
            "WORKER_CMD_END_FENCE {WORKER_CMD_END_FENCE:?} != retail vertex writers {fence:?}"
        ));
    }
    Ok(())
}

pub fn worker_cmd_nested_in_present() -> Result<(), String> {
    let mut app = probe_app();
    app.init_resource::<Trace>();
    configure_client_sets(&mut app);
    install_client_probes(&mut app);
    app.add_systems(
        Update,
        push("W:cell_static").in_set(WorkerCmdSet::CellStatic),
    );
    app.add_systems(Update, push("W:skin_model").in_set(WorkerCmdSet::SkinModel));
    app.world_mut().run_schedule(Update);
    let trace = app.world().resource::<Trace>().0.clone();
    let send = trace
        .iter()
        .position(|s| *s == "C:Send")
        .ok_or_else(|| format!("no C:Send in {trace:?}"))?;
    let ui = trace
        .iter()
        .position(|s| *s == "C:Ui")
        .ok_or_else(|| format!("no C:Ui in {trace:?}"))?;
    let cell = trace
        .iter()
        .position(|s| *s == "W:cell_static")
        .ok_or_else(|| format!("cell_static probe did not run: {trace:?}"))?;
    let skin = trace
        .iter()
        .position(|s| *s == "W:skin_model")
        .ok_or_else(|| format!("skin_model probe did not run: {trace:?}"))?;
    if !(send < cell && cell < ui) {
        return Err(format!(
            "cell_static not inside Present (after Send, before Ui): {trace:?}"
        ));
    }
    if !(send < skin && skin < ui) {
        return Err(format!(
            "skin_model not inside Present (after Send, before Ui): {trace:?}"
        ));
    }
    Ok(())
}

pub fn role(role: RuntimeRole, expect_authority: bool, expect_client: bool) -> Result<(), String> {
    let mut app = probe_app();
    app.init_resource::<Trace>();
    match role {
        RuntimeRole::Listen => app.add_plugins(NetPlugin::listen()),
        RuntimeRole::Dedicated => app.add_plugins(NetPlugin::dedicated()),
        RuntimeRole::Client => app.add_plugins(NetPlugin::client()),
        RuntimeRole::Replay => app.add_plugins(NetPlugin::replay()),
    };
    if expect_authority {
        install_authority_probes(&mut app);
    }
    if expect_client {
        install_client_probes(&mut app);
    }
    if expect_authority {
        app.world_mut().run_schedule(FixedUpdate);
    }
    if expect_client {
        app.world_mut().run_schedule(Update);
    }

    let trace = app.world().resource::<Trace>().0.clone();
    let has_authority = trace.iter().any(|l| l.starts_with("A:"));
    let has_client = trace.iter().any(|l| l.starts_with("C:"));
    if has_authority != expect_authority {
        return Err(format!(
            "{role:?}: authority execution mismatch (trace={trace:?})"
        ));
    }
    if has_client != expect_client {
        return Err(format!(
            "{role:?}: client execution mismatch (trace={trace:?})"
        ));
    }
    let got = *app.world().resource::<RuntimeRole>();
    if got != role {
        return Err(format!("{role:?}: RuntimeRole resource is {got:?}"));
    }
    Ok(())
}
