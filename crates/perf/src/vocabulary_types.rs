#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span {
    FixedTocAdvance,
    FixedTocBookkeeping,
    FixedTocFanout,
    FixedTocGather,
    FixedTocIngress,
    FixedTocSnapshot,
    FixedTocStep,
    FramesDiagMs,
    FramesEffectsMs,
    FramesFixedMs,
    FramesPostupdateMs,
    FramesPredictMs,
    FramesPresentMs,
    FramesPreupdateMs,
    FramesUpdateMs,
    FramesWallFrameMs,
    HostFxPresentCpuMs,
    HostFxUpdateCpuMs,
    HostPostExecuteMs,
    HostPostRebuildMs,
    HostSkinModelMs,
    RenderColourSubmitMs,
    RenderCullCpuMs,
    RenderRenderExtractWaitMs,
    RenderRenderRenderMs,
    RenderRenderThreadMs,
    TocInput,
    TocLoad,
    TocReceive,
    TocReconcile,
    TocSend,
    TocUi,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counter {
    CounterBindGroup0,

    CounterBindGroup1,

    CounterCmdState,

    CounterMultiDraws,
    CounterMultiDrawCommands,

    CounterDipsColour,

    CounterDipsSun,
    CounterDraws,
    CounterFxElemAllocFail,
    CounterFxElemLive,
    CounterProcessAllocations,
    CounterSubmittedBatches,

    RenderGraphRenderMs,
    RenderGraphSubmitMs,
    RenderGraphPresentMs,

    RenderSubmitSunMs,
    RenderSubmitGatherMs,
    RenderSubmitPrepareMs,
    RenderSubmitArenaMs,
    RenderSubmitRecordMs,

    RenderGpuColourMs,

    RenderGpuSunMs,

    RenderGpuSpotMs,

    RenderGpuFloatzMs,

    RenderGpuPostfxMs,

    RenderGpuFrameMs,
    SpotShadowGpu,
    SpotShadowGpuMiss,
    SpotShadowSlotN,
    XmodelColourCameraFrustum,
    XmodelColourNoLighting,

    XmodelLayoutOverlay,

    FxLayoutOverlay,

    WorldPretessSkip,

    SmodelIbSkip,
}

impl Span {
    pub const COUNT: usize = Self::TocUi as usize + 1;

    /// Every span, in declaration order. The report walks this; the recorder
    /// indexes its arrays by `span as usize`, so the two must not drift.
    pub const ALL: [Self; Self::COUNT] = [
        Self::FixedTocAdvance,
        Self::FixedTocBookkeeping,
        Self::FixedTocFanout,
        Self::FixedTocGather,
        Self::FixedTocIngress,
        Self::FixedTocSnapshot,
        Self::FixedTocStep,
        Self::FramesDiagMs,
        Self::FramesEffectsMs,
        Self::FramesFixedMs,
        Self::FramesPostupdateMs,
        Self::FramesPredictMs,
        Self::FramesPresentMs,
        Self::FramesPreupdateMs,
        Self::FramesUpdateMs,
        Self::FramesWallFrameMs,
        Self::HostFxPresentCpuMs,
        Self::HostFxUpdateCpuMs,
        Self::HostPostExecuteMs,
        Self::HostPostRebuildMs,
        Self::HostSkinModelMs,
        Self::RenderColourSubmitMs,
        Self::RenderCullCpuMs,
        Self::RenderRenderExtractWaitMs,
        Self::RenderRenderRenderMs,
        Self::RenderRenderThreadMs,
        Self::TocInput,
        Self::TocLoad,
        Self::TocReceive,
        Self::TocReconcile,
        Self::TocSend,
        Self::TocUi,
    ];

    /// The name the span carries in a trace and in the bench report. It is the
    /// Perfetto event name without the `span.` track prefix.
    pub const fn name(self) -> &'static str {
        match self {
            Self::FixedTocAdvance => "Advance",
            Self::FixedTocBookkeeping => "Bookkeeping",
            Self::FixedTocFanout => "Fanout",
            Self::FixedTocGather => "Gather",
            Self::FixedTocIngress => "Ingress",
            Self::FixedTocSnapshot => "Snapshot",
            Self::FixedTocStep => "Step",
            Self::FramesDiagMs => "Diag",
            Self::FramesEffectsMs => "Effects",
            Self::FramesFixedMs => "FixedUpdate",
            Self::FramesPostupdateMs => "PostUpdate",
            Self::FramesPredictMs => "Predict",
            Self::FramesPresentMs => "Present",
            Self::FramesPreupdateMs => "PreUpdate",
            Self::FramesUpdateMs => "Update",
            Self::FramesWallFrameMs => "wall",
            Self::HostFxPresentCpuMs => "fx_present",
            Self::HostFxUpdateCpuMs => "fx_update",
            Self::HostPostExecuteMs => "post_execute",
            Self::HostPostRebuildMs => "post_rebuild",
            Self::HostSkinModelMs => "skin_model",
            Self::RenderColourSubmitMs => "colour_submit",
            Self::RenderCullCpuMs => "cull",
            Self::RenderRenderExtractWaitMs => "extract_wait",
            Self::RenderRenderRenderMs => "render_render",
            Self::RenderRenderThreadMs => "render_thread",
            Self::TocInput => "Input",
            Self::TocLoad => "Load",
            Self::TocReceive => "Receive",
            Self::TocReconcile => "Reconcile",
            Self::TocSend => "Send",
            Self::TocUi => "Ui",
        }
    }
}

/// What a counter's value means, so the report can label it and refuse to add
/// milliseconds to draw calls. A counter is one or the other for its whole
/// life; there is no counter whose unit depends on the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// A duration, emitted in milliseconds.
    Milliseconds,
    /// A count of things that happened in the frame the sample belongs to.
    Count,
}

impl Counter {
    pub const COUNT: usize = Self::SmodelIbSkip as usize + 1;

    /// Every counter, in declaration order. The recorder indexes its arrays by
    /// `counter as usize`, so this and the enum must not drift.
    pub const ALL: [Self; Self::COUNT] = [
        Self::CounterBindGroup0,
        Self::CounterBindGroup1,
        Self::CounterCmdState,
        Self::CounterMultiDraws,
        Self::CounterMultiDrawCommands,
        Self::CounterDipsColour,
        Self::CounterDipsSun,
        Self::CounterDraws,
        Self::CounterFxElemAllocFail,
        Self::CounterFxElemLive,
        Self::CounterProcessAllocations,
        Self::CounterSubmittedBatches,
        Self::RenderGraphRenderMs,
        Self::RenderGraphSubmitMs,
        Self::RenderGraphPresentMs,
        Self::RenderSubmitSunMs,
        Self::RenderSubmitGatherMs,
        Self::RenderSubmitPrepareMs,
        Self::RenderSubmitArenaMs,
        Self::RenderSubmitRecordMs,
        Self::RenderGpuColourMs,
        Self::RenderGpuSunMs,
        Self::RenderGpuSpotMs,
        Self::RenderGpuFloatzMs,
        Self::RenderGpuPostfxMs,
        Self::RenderGpuFrameMs,
        Self::SpotShadowGpu,
        Self::SpotShadowGpuMiss,
        Self::SpotShadowSlotN,
        Self::XmodelColourCameraFrustum,
        Self::XmodelColourNoLighting,
        Self::XmodelLayoutOverlay,
        Self::FxLayoutOverlay,
        Self::WorldPretessSkip,
        Self::SmodelIbSkip,
    ];

    /// The name the counter carries in a trace and in the bench report.
    pub const fn name(self) -> &'static str {
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
            Self::RenderGraphPresentMs => "graph_present",
            Self::RenderSubmitSunMs => "submit_sun",
            Self::RenderSubmitGatherMs => "submit_gather",
            Self::RenderSubmitPrepareMs => "submit_prepare",
            Self::RenderSubmitArenaMs => "submit_arena",
            Self::RenderSubmitRecordMs => "submit_record",
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

    /// The unit the emitter passes. `emit` takes an `f64` either way, so this
    /// is the only thing that tells a millisecond from a draw call.
    pub const fn unit(self) -> Unit {
        match self {
            Self::RenderGraphRenderMs
            | Self::RenderGraphSubmitMs
            | Self::RenderGraphPresentMs
            | Self::RenderSubmitSunMs
            | Self::RenderSubmitGatherMs
            | Self::RenderSubmitPrepareMs
            | Self::RenderSubmitArenaMs
            | Self::RenderSubmitRecordMs
            | Self::RenderGpuColourMs
            | Self::RenderGpuSunMs
            | Self::RenderGpuSpotMs
            | Self::RenderGpuFloatzMs
            | Self::RenderGpuPostfxMs
            | Self::RenderGpuFrameMs => Unit::Milliseconds,
            _ => Unit::Count,
        }
    }

    /// Where the number was measured. A GPU counter is a timestamp the driver
    /// resolved some frames after the CPU one next to it, which is why the
    /// report never puts the two in the same total.
    pub const fn origin(self) -> Origin {
        match self {
            Self::RenderGpuColourMs
            | Self::RenderGpuSunMs
            | Self::RenderGpuSpotMs
            | Self::RenderGpuFloatzMs
            | Self::RenderGpuPostfxMs
            | Self::RenderGpuFrameMs
            | Self::SpotShadowGpu
            | Self::SpotShadowGpuMiss => Origin::Gpu,
            _ => Origin::Cpu,
        }
    }
}

/// Which side of the device a counter was measured on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Cpu,
    Gpu,
}
