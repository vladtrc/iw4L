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
    HostStaticSunFxMs,
    HostStaticSunMs,
    RenderColourPrepareMs,
    RenderColourSubmitMs,
    RenderCullCpuMs,
    RenderPrepareCameraMs,
    RenderPrepareShadowMs,
    RenderReceiveWorldMs,
    RenderExtractBodyMs,
    RenderDispatchWorldMs,
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
    /// The wall between the last system of the render graph's `Render` set and
    /// the first of its `Finish` set: bevy's `submit_pending_command_buffers`
    /// — which finishes every pending encoder *and* calls `Queue::submit` —
    /// plus `handle_uncovered_swap_chains`, plus whatever the executor ran in
    /// the gaps. Not a `Queue::submit` body, and not addable to anything.
    RenderGraphSubmitIntervalMs,
    /// Command buffers and unfinished encoders waiting when that interval
    /// began: what the one number above had to finish and submit.
    RenderGraphSubmitPendingN,
    RenderGraphPresentMs,
    /// How long the state this render frame drew had been extracted by the
    /// time the render graph finished with it, and how many main frames the
    /// main world had opened since. Zero frames behind is a render world
    /// running in line with the main one; one or more is a pipelined render
    /// world drawing an older state. Neither is input-to-photon: it stops
    /// where the graph does, one step before the present call, and the
    /// compositor and the display are further outside still.
    RenderPresentedStateAgeMs,
    RenderPresentedFramesBehind,

    /// Code-constant writes the material overlay made for the colour list.
    /// Work the frame did to fill values a shader may or may not read; it
    /// falls when a run knows what its shell reads before filling them.
    CounterOverlayConstWrites,

    RenderSubmitSunMs,
    RenderSubmitGatherMs,
    RenderSubmitPrepareMs,
    RenderSubmitArenaMs,
    RenderSubmitRecordMs,

    PresentPublishMs,
    HudSurfacesScheduleMs,
    HudStageMaxScheduleMs,
    HudTessBodyMs,
    HudTessJobs,
    UiHudSetupMs,
    UiApplyDeferredMs,
    UiHudVisibilityMs,
    HudStageMaxScheduleAt,

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

    /// Bevy `PrepareViews` set as a wall interval: the render systems between
    /// `Specialize` and `Queue` ran inside it, and so did whatever else the
    /// executor scheduled there. A body only in the sense the HUD bodies are —
    /// read it against the schedule gaps, never as exclusive CPU work.
    RenderPrepareViewsMs,

    /// Constant-arena upload census, taken where the upload is chosen and
    /// performed. Byte counts are CPU-side staging sizes; the driver copies
    /// them to the device after the call returns.
    CounterArenaUsedBytes,
    CounterArenaCapacityBytes,
    CounterArenaDirtyBytes,
    CounterArenaUploadedBytes,
    CounterArenaUploadCalls,
    /// Full uploads grouped by reason: the GPU buffer was recreated, or an
    /// explicit owner/generation/reservation reset required it. A changed
    /// logical length alone is not a reason — the tail uploads on its own.
    CounterArenaFullResize,
    CounterArenaFullFlag,
    CounterArenaReallocN,

    /// Postfx submit decision: the refusal discriminant (no strings per
    /// frame), and the planned/executed step counts with the frame identity
    /// held at the decision point.
    CounterPostFxRefusal,
    CounterPostFxPlannedSteps,
    CounterPostFxExecutedSteps,
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
        Self::HostStaticSunFxMs,
        Self::HostStaticSunMs,
        Self::RenderColourPrepareMs,
        Self::RenderColourSubmitMs,
        Self::RenderCullCpuMs,
        Self::RenderPrepareCameraMs,
        Self::RenderPrepareShadowMs,
        Self::RenderReceiveWorldMs,
        Self::RenderExtractBodyMs,
        Self::RenderDispatchWorldMs,
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
            Self::HostStaticSunFxMs => "static_sun_fx",
            Self::HostStaticSunMs => "static_sun",
            Self::RenderColourPrepareMs => "colour_prepare",
            Self::RenderColourSubmitMs => "colour_submit",
            Self::RenderCullCpuMs => "cull",
            Self::RenderPrepareCameraMs => "prepare_camera",
            Self::RenderPrepareShadowMs => "prepare_shadow",
            Self::RenderReceiveWorldMs => "receive_render_world",
            Self::RenderExtractBodyMs => "extract_body",
            Self::RenderDispatchWorldMs => "dispatch_render_world",
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

    /// Whether this span is a top-level scope of the frame, declared here
    /// rather than observed.
    ///
    /// The frame's coverage is the union of these, clipped to the wall, and
    /// `wall - covered` is the time nothing accounted for. Reading "was
    /// anything else open under it on this thread" instead would make the
    /// answer depend on which worker the executor ran a `begin` and its `end`
    /// on: `PreUpdate` opens in `First` and closes in `RunFixedMainLoop`, and
    /// when those land on different threads the span is root on neither — its
    /// whole interval falls out of the union and the remainder counts it as
    /// unclassified.
    ///
    /// The four schedule spans do not overlap each other and the render
    /// thread's does not nest in any of them, so their union is the frame's
    /// covered time whatever thread each was observed on.
    pub const fn coverage_root(self) -> bool {
        matches!(
            self,
            Self::FramesPreupdateMs
                | Self::FramesFixedMs
                | Self::FramesUpdateMs
                | Self::FramesPostupdateMs
                | Self::RenderRenderThreadMs
        )
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
    pub const COUNT: usize = Self::CounterPostFxExecutedSteps as usize + 1;

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
        Self::RenderGraphSubmitIntervalMs,
        Self::RenderGraphSubmitPendingN,
        Self::RenderGraphPresentMs,
        Self::RenderPresentedStateAgeMs,
        Self::RenderPresentedFramesBehind,
        Self::CounterOverlayConstWrites,
        Self::RenderSubmitSunMs,
        Self::RenderSubmitGatherMs,
        Self::RenderSubmitPrepareMs,
        Self::RenderSubmitArenaMs,
        Self::RenderSubmitRecordMs,
        Self::PresentPublishMs,
        Self::HudSurfacesScheduleMs,
        Self::HudStageMaxScheduleMs,
        Self::HudTessBodyMs,
        Self::HudTessJobs,
        Self::UiHudSetupMs,
        Self::UiApplyDeferredMs,
        Self::UiHudVisibilityMs,
        Self::HudStageMaxScheduleAt,
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
        Self::RenderPrepareViewsMs,
        Self::CounterArenaUsedBytes,
        Self::CounterArenaCapacityBytes,
        Self::CounterArenaDirtyBytes,
        Self::CounterArenaUploadedBytes,
        Self::CounterArenaUploadCalls,
        Self::CounterArenaFullResize,
        Self::CounterArenaFullFlag,
        Self::CounterArenaReallocN,
        Self::CounterPostFxRefusal,
        Self::CounterPostFxPlannedSteps,
        Self::CounterPostFxExecutedSteps,
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
            Self::RenderGraphSubmitIntervalMs => "graph_submit_schedule_interval",
            Self::RenderGraphSubmitPendingN => "graph_submit_pending",
            Self::RenderGraphPresentMs => "graph_present",
            Self::RenderPresentedStateAgeMs => "presented_state_age",
            Self::RenderPresentedFramesBehind => "presented_frames_behind",
            Self::CounterOverlayConstWrites => "overlay_const_writes",
            Self::RenderSubmitSunMs => "submit_sun",
            Self::RenderSubmitGatherMs => "submit_gather",
            Self::RenderSubmitPrepareMs => "submit_prepare",
            Self::RenderSubmitArenaMs => "submit_arena",
            Self::RenderSubmitRecordMs => "submit_record",
            Self::PresentPublishMs => "present_publish",
            Self::HudSurfacesScheduleMs => "hud_surfaces_schedule_interval",
            Self::HudStageMaxScheduleMs => "hud_stage_max_schedule_interval",
            Self::HudTessBodyMs => "hud_tess_body",
            Self::HudTessJobs => "hud_tess_jobs",
            Self::UiHudSetupMs => "ui_hud_setup",
            Self::UiApplyDeferredMs => "ui_apply_deferred",
            Self::UiHudVisibilityMs => "ui_hud_visibility",
            Self::HudStageMaxScheduleAt => "hud_stage_max_schedule_interval_at",
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
            Self::RenderPrepareViewsMs => "prepare_views",
            Self::CounterArenaUsedBytes => "arena_used_bytes",
            Self::CounterArenaCapacityBytes => "arena_capacity_bytes",
            Self::CounterArenaDirtyBytes => "arena_dirty_bytes",
            Self::CounterArenaUploadedBytes => "arena_uploaded_bytes",
            Self::CounterArenaUploadCalls => "arena_upload_calls",
            Self::CounterArenaFullResize => "arena_full_resize",
            Self::CounterArenaFullFlag => "arena_full_flag",
            Self::CounterArenaReallocN => "arena_realloc",
            Self::CounterPostFxRefusal => "postfx_refusal",
            Self::CounterPostFxPlannedSteps => "postfx_planned_steps",
            Self::CounterPostFxExecutedSteps => "postfx_executed_steps",
        }
    }

    /// The unit the emitter passes. `emit` takes an `f64` either way, so this
    /// is the only thing that tells a millisecond from a draw call.
    pub const fn unit(self) -> Unit {
        match self {
            Self::RenderGraphRenderMs
            | Self::RenderGraphSubmitIntervalMs
            | Self::RenderGraphPresentMs
            | Self::RenderPresentedStateAgeMs
            | Self::RenderSubmitSunMs
            | Self::RenderSubmitGatherMs
            | Self::RenderSubmitPrepareMs
            | Self::RenderSubmitArenaMs
            | Self::RenderSubmitRecordMs
            | Self::PresentPublishMs
            | Self::HudSurfacesScheduleMs
            | Self::HudStageMaxScheduleMs
            | Self::HudTessBodyMs
            | Self::UiHudSetupMs
            | Self::UiApplyDeferredMs
            | Self::UiHudVisibilityMs
            | Self::RenderGpuColourMs
            | Self::RenderGpuSunMs
            | Self::RenderGpuSpotMs
            | Self::RenderGpuFloatzMs
            | Self::RenderGpuPostfxMs
            | Self::RenderGpuFrameMs
            | Self::RenderPrepareViewsMs => Unit::Milliseconds,
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
