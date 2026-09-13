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
