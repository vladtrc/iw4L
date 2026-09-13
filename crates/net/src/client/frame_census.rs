use std::fmt::Write as _;
use std::time::Instant;

use bevy::prelude::*;

use crate::schedule::ClientSet;
use frame::{CLIENT_TOC, ClientEdge, client_set_name};

#[derive(Resource, Debug, Clone, Default)]
pub struct ClientPhaseCensus {
    pub n: [Option<u64>; CLIENT_TOC.len()],

    pub bytes: [Option<u64>; CLIENT_TOC.len()],
    seam_n: Option<u64>,
    seam_bytes: Option<u64>,
}

impl ClientPhaseCensus {
    pub fn alloc_n(&self) -> Option<String> {
        format_toc_u64(&self.n)
    }

    pub fn alloc_bytes(&self) -> Option<String> {
        format_toc_u64(&self.bytes)
    }

    pub fn alloc_sum_n(&self) -> Option<u64> {
        sum_named(&self.n)
    }

    pub fn alloc_sum_bytes(&self) -> Option<u64> {
        sum_named(&self.bytes)
    }
}

fn format_toc_u64(values: &[Option<u64>; CLIENT_TOC.len()]) -> Option<String> {
    let mut out = String::new();
    for (index, phase) in CLIENT_TOC.iter().enumerate() {
        let Some(n) = values[index] else { continue };
        if !out.is_empty() {
            out.push(',');
        }
        let _ = write!(out, "{}:{n}", client_set_name(phase));
    }
    (!out.is_empty()).then_some(out)
}

fn sum_named(values: &[Option<u64>; CLIENT_TOC.len()]) -> Option<u64> {
    let mut sum = 0u64;
    let mut any = false;
    for slot in values {
        let Some(n) = slot else { continue };
        sum += *n;
        any = true;
    }
    any.then_some(sum)
}

fn stamp_client_edge<const EDGE: u8>(mut census: ResMut<ClientPhaseCensus>) {
    let alloc = diag::process_allocations();
    if EDGE > 0 {
        let slot = EDGE as usize - 1;
        client_toc_span(slot).end();
        if let Some(n0) = census.seam_n {
            census.n[slot] = Some(alloc.process_allocations.saturating_sub(n0));
        }
        if let Some(b0) = census.seam_bytes {
            census.bytes[slot] = Some(alloc.process_allocation_bytes.saturating_sub(b0));
        }
    }
    if EDGE == 0 {
        perf::Span::FramesUpdateMs.begin();
    }
    if (EDGE as usize) < CLIENT_TOC.len() {
        client_toc_span(EDGE as usize).begin();
    }
    if EDGE as usize == CLIENT_TOC.len() {
        perf::Span::FramesUpdateMs.end();
    }
    census.seam_n = Some(alloc.process_allocations);
    census.seam_bytes = Some(alloc.process_allocation_bytes);
}

fn client_toc_span(slot: usize) -> perf::Span {
    match slot {
        0 => perf::Span::TocLoad,
        1 => perf::Span::TocReceive,
        2 => perf::Span::TocReconcile,
        3 => perf::Span::TocInput,
        4 => perf::Span::FramesPredictMs,
        5 => perf::Span::TocSend,
        6 => perf::Span::FramesPresentMs,
        7 => perf::Span::TocUi,
        8 => perf::Span::FramesEffectsMs,
        9 => perf::Span::FramesDiagMs,
        _ => unreachable!("CLIENT_TOC slot {slot}"),
    }
}

fn register_phase_census(app: &mut App) {
    app.init_resource::<ClientPhaseCensus>();
    macro_rules! seams {
        ($($edge:literal),* $(,)?) => {$(
            app.add_systems(
                Update,
                stamp_client_edge::<$edge>.in_set(ClientEdge($edge)),
            );
        )*};
    }
    seams!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
}

pub const HUD_STAGE_N: usize = 9;

#[derive(Resource, Debug, Default, Clone)]
pub struct UpdatePhaseCensus {
    pub present_started: Option<Instant>,

    pub hud_root_setup_ms: Option<f32>,

    pub present_apply_deferred_ms: Option<f32>,

    pub hud_visibility_ms: Option<f32>,

    pub hud_surfaces_ms: Option<f32>,

    pub hud_stage_ms: [Option<f32>; HUD_STAGE_N],

    pub publish_presented_ms: Option<f32>,
}

fn begin_update_census(mut census: ResMut<UpdatePhaseCensus>) {
    *census = UpdatePhaseCensus::default();
}

fn begin_present_census(mut census: ResMut<UpdatePhaseCensus>) {
    census.present_started = Some(Instant::now());
}

pub fn register_update_phase_census(app: &mut App) {
    register_phase_census(app);
    app.init_resource::<UpdatePhaseCensus>().add_systems(
        Update,
        (
            begin_update_census.before(ClientSet::Load),
            begin_present_census.in_set(ClientSet::Send),
        ),
    );
}
