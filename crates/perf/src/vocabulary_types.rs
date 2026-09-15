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
