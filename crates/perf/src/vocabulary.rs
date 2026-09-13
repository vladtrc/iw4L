use std::sync::OnceLock;

use perfetto_sdk::track_event::{EventContext, TrackEventCounter, TrackEventTrack};

perfetto_sdk::track_event_categories! {
    pub(crate) mod vocabulary_te_ns {
        ("iw4l.frame", "IW4L iw4l.frame events", ["iw4l"]),
        ("iw4l.fx", "IW4L iw4l.fx events", ["iw4l"]),
        ("iw4l.render", "IW4L iw4l.render events", ["iw4l"]),
        ("iw4l.sim", "IW4L iw4l.sim events", ["iw4l"]),
    }
}

pub(crate) use vocabulary_te_ns as perfetto_te_ns;

pub(crate) fn register_categories() {
    let _ = vocabulary_te_ns::register();
}

pub use crate::vocabulary_types::Span;

const SPAN_COUNT: usize = Span::TocUi as usize + 1;
static SPAN_TRACKS: [OnceLock<TrackEventTrack>; SPAN_COUNT] =
    [const { OnceLock::new() }; SPAN_COUNT];

impl Span {
    fn track(self) -> &'static TrackEventTrack {
        SPAN_TRACKS[self as usize].get_or_init(|| {
            TrackEventTrack::register_named_track(
                self.track_name(),
                self as u64,
                TrackEventTrack::process_track_uuid(),
            )
            .expect("register Perfetto span track")
        })
    }

    fn track_name(self) -> &'static str {
        match self {
            Self::FixedTocAdvance => "span.Advance",
            Self::FixedTocBookkeeping => "span.Bookkeeping",
            Self::FixedTocFanout => "span.Fanout",
            Self::FixedTocGather => "span.Gather",
            Self::FixedTocIngress => "span.Ingress",
            Self::FixedTocSnapshot => "span.Snapshot",
            Self::FixedTocStep => "span.Step",
            Self::FramesDiagMs => "span.Diag",
            Self::FramesEffectsMs => "span.Effects",
            Self::FramesFixedMs => "span.FixedUpdate",
            Self::FramesPostupdateMs => "span.PostUpdate",
            Self::FramesPredictMs => "span.Predict",
            Self::FramesPresentMs => "span.Present",
            Self::FramesPreupdateMs => "span.PreUpdate",
            Self::FramesUpdateMs => "span.Update",
            Self::FramesWallFrameMs => "span.wall",
            Self::HostFxPresentCpuMs => "span.fx_present",
            Self::HostFxUpdateCpuMs => "span.fx_update",
            Self::HostPostExecuteMs => "span.post_execute",
            Self::HostPostRebuildMs => "span.post_rebuild",
            Self::HostSkinModelMs => "span.skin_model",
            Self::RenderColourSubmitMs => "span.colour_submit",
            Self::RenderCullCpuMs => "span.cull",
            Self::RenderRenderExtractWaitMs => "span.extract_wait",
            Self::RenderRenderRenderMs => "span.render_render",
            Self::RenderRenderThreadMs => "span.render_thread",
            Self::TocInput => "span.Input",
            Self::TocLoad => "span.Load",
            Self::TocReceive => "span.Receive",
            Self::TocReconcile => "span.Reconcile",
            Self::TocSend => "span.Send",
            Self::TocUi => "span.Ui",
        }
    }

    #[inline]
    pub fn begin(self) {
        macro_rules! begin {
            ($category:literal, $name:literal) => {
                perfetto_sdk::track_event_begin!($category, $name, |ctx: &mut EventContext| {
                    ctx.set_track(self.track());
                })
            };
        }
        match self {
            Self::FixedTocAdvance => begin!("iw4l.sim", "Advance"),
            Self::FixedTocBookkeeping => begin!("iw4l.sim", "Bookkeeping"),
            Self::FixedTocFanout => begin!("iw4l.sim", "Fanout"),
            Self::FixedTocGather => begin!("iw4l.sim", "Gather"),
            Self::FixedTocIngress => begin!("iw4l.sim", "Ingress"),
            Self::FixedTocSnapshot => begin!("iw4l.sim", "Snapshot"),
            Self::FixedTocStep => begin!("iw4l.sim", "Step"),
            Self::FramesDiagMs => begin!("iw4l.sim", "Diag"),
            Self::FramesEffectsMs => begin!("iw4l.sim", "Effects"),
            Self::FramesFixedMs => begin!("iw4l.sim", "FixedUpdate"),
            Self::FramesPostupdateMs => begin!("iw4l.frame", "PostUpdate"),
            Self::FramesPredictMs => begin!("iw4l.sim", "Predict"),
            Self::FramesPresentMs => begin!("iw4l.sim", "Present"),
            Self::FramesPreupdateMs => begin!("iw4l.frame", "PreUpdate"),
            Self::FramesUpdateMs => begin!("iw4l.sim", "Update"),
            Self::FramesWallFrameMs => begin!("iw4l.frame", "wall"),
            Self::HostFxPresentCpuMs => begin!("iw4l.fx", "fx_present"),
            Self::HostFxUpdateCpuMs => begin!("iw4l.fx", "fx_update"),
            Self::HostPostExecuteMs => begin!("iw4l.render", "post_execute"),
            Self::HostPostRebuildMs => begin!("iw4l.render", "post_rebuild"),
            Self::HostSkinModelMs => begin!("iw4l.render", "skin_model"),
            Self::RenderColourSubmitMs => begin!("iw4l.render", "colour_submit"),
            Self::RenderCullCpuMs => begin!("iw4l.render", "cull"),
            Self::RenderRenderExtractWaitMs => begin!("iw4l.render", "extract_wait"),
            Self::RenderRenderRenderMs => begin!("iw4l.render", "render_render"),
            Self::RenderRenderThreadMs => begin!("iw4l.render", "render_thread"),
            Self::TocInput => begin!("iw4l.sim", "Input"),
            Self::TocLoad => begin!("iw4l.sim", "Load"),
            Self::TocReceive => begin!("iw4l.sim", "Receive"),
            Self::TocReconcile => begin!("iw4l.sim", "Reconcile"),
            Self::TocSend => begin!("iw4l.sim", "Send"),
            Self::TocUi => begin!("iw4l.sim", "Ui"),
        }
    }
    #[inline]
    pub fn end(self) {
        macro_rules! end {
            ($category:literal) => {
                perfetto_sdk::track_event_end!($category, |ctx: &mut EventContext| {
                    ctx.set_track(self.track());
                })
            };
        }
        match self {
            Self::FixedTocAdvance
            | Self::FixedTocBookkeeping
            | Self::FixedTocFanout
            | Self::FixedTocGather
            | Self::FixedTocIngress
            | Self::FixedTocSnapshot
            | Self::FixedTocStep
            | Self::FramesDiagMs
            | Self::FramesEffectsMs
            | Self::FramesFixedMs
            | Self::FramesPredictMs
            | Self::FramesPresentMs
            | Self::FramesUpdateMs
            | Self::TocInput
            | Self::TocLoad
            | Self::TocReceive
            | Self::TocReconcile
            | Self::TocSend
            | Self::TocUi => end!("iw4l.sim"),
            Self::FramesPostupdateMs | Self::FramesPreupdateMs | Self::FramesWallFrameMs => {
                end!("iw4l.frame")
            }
            Self::HostFxPresentCpuMs | Self::HostFxUpdateCpuMs => end!("iw4l.fx"),
            Self::HostPostExecuteMs
            | Self::HostPostRebuildMs
            | Self::HostSkinModelMs
            | Self::RenderColourSubmitMs
            | Self::RenderCullCpuMs
            | Self::RenderRenderExtractWaitMs
            | Self::RenderRenderRenderMs
            | Self::RenderRenderThreadMs => end!("iw4l.render"),
        }
    }
    #[inline]
    pub fn enter(self) -> SpanGuard {
        self.begin();
        SpanGuard { span: self }
    }
}

#[must_use = "the span closes when this guard drops"]
pub struct SpanGuard {
    span: Span,
}

impl Drop for SpanGuard {
    #[inline]
    fn drop(&mut self) {
        self.span.end();
    }
}

pub use crate::vocabulary_types::Counter;

const COUNTER_COUNT: usize = Counter::SmodelIbSkip as usize + 1;
static COUNTER_TRACKS: [OnceLock<TrackEventTrack>; COUNTER_COUNT] =
    [const { OnceLock::new() }; COUNTER_COUNT];

#[derive(Clone, Copy, PartialEq, Eq)]
enum CounterCategory {
    Frame,
    Fx,
    Render,
}

impl Counter {
    fn track_name(self) -> &'static str {
        match self {
            Self::CounterBindGroup0 => "bind0",
            Self::CounterBindGroup1 => "bind1",
            Self::CounterCmdState => "cmd_state",
            Self::CounterMultiDraws => "multi_draw",
            Self::CounterMultiDrawCommands => "multi_draw_cmds",
            Self::CounterDipsColour => "dip_colour",
            Self::CounterDipsSun => "dip_sun",
            Self::CounterDraws => "draws",
            Self::CounterFxElemAllocFail => "fx_elem_alloc_fail",
            Self::CounterFxElemLive => "fx_elem_live",
            Self::CounterProcessAllocations => "process_allocations",
            Self::CounterSubmittedBatches => "batches",
            Self::RenderGraphRenderMs => "graph_render",
            Self::RenderGraphSubmitMs => "graph_submit",
            Self::RenderSubmitSunMs => "submit_sun",
            Self::RenderSubmitGatherMs => "submit_gather",
            Self::RenderSubmitPrepareMs => "submit_prepare",
            Self::RenderSubmitArenaMs => "submit_arena",
            Self::RenderSubmitRecordMs => "submit_record",
            Self::RenderGraphPresentMs => "graph_present",
            Self::RenderGpuColourMs => "gpu_colour",
            Self::RenderGpuSunMs => "gpu_sun",
            Self::RenderGpuSpotMs => "gpu_spot",
            Self::RenderGpuFloatzMs => "gpu_floatz",
            Self::RenderGpuPostfxMs => "gpu_postfx",
            Self::RenderGpuFrameMs => "gpu_frame",
            Self::SpotShadowGpu => "spot_shadow_gpu",
            Self::SpotShadowGpuMiss => "spot_shadow_gpu_miss",
            Self::SpotShadowSlotN => "spot_shadow_slot_n",
            Self::XmodelColourCameraFrustum => "xmodel_colour_camera_frustum",
            Self::XmodelColourNoLighting => "xmodel_colour_no_lighting",
            Self::XmodelLayoutOverlay => "xmodel_layout_overlay",
            Self::FxLayoutOverlay => "fx_layout_overlay",
            Self::WorldPretessSkip => "world_pretess_skip",
            Self::SmodelIbSkip => "smodel_ib_skip",
        }
    }

    fn category(self) -> CounterCategory {
        match self {
            Self::CounterFxElemAllocFail | Self::CounterFxElemLive => CounterCategory::Fx,
            Self::CounterProcessAllocations => CounterCategory::Frame,
            _ => CounterCategory::Render,
        }
    }

    fn track(self) -> &'static TrackEventTrack {
        COUNTER_TRACKS[self as usize].get_or_init(|| {
            TrackEventTrack::register_counter_track(
                self.track_name(),
                TrackEventTrack::process_track_uuid(),
            )
            .expect("register Perfetto counter track")
        })
    }

    #[inline]
    pub fn emit(self, value: f64) {
        macro_rules! emit {
            ($category:literal) => {{
                if !perfetto_sdk::track_event_category_enabled!($category) {
                    return;
                }
                let track = self.track();
                perfetto_sdk::track_event_counter!($category, |ctx: &mut EventContext| {
                    ctx.set_track(track);
                    ctx.set_counter(TrackEventCounter::Double(value));
                })
            }};
        }
        match self.category() {
            CounterCategory::Frame => emit!("iw4l.frame"),
            CounterCategory::Fx => emit!("iw4l.fx"),
            CounterCategory::Render => emit!("iw4l.render"),
        }
    }
}
