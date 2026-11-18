use std::collections::VecDeque;
use std::sync::Mutex;

use crate::cue::{CueFailure, CueSubmission};
use crate::cue_execution::CueCancellation;
use crate::render_core::AudioScope;
use crate::runtime::LOGICAL_INSTANCES;
use crate::sources::{DesiredSource, SourceKey, SourcePublisher, SourceScene};

pub(crate) enum CueCommand {
    Start(Box<CueSubmission>),
    StopScope(AudioScope),
}

struct InboxState {
    commands: VecDeque<CueCommand>,
    starts: usize,
    resets: usize,
    publisher: SourcePublisher,
    scene: Option<SourceScene>,
}

pub(crate) struct AudioInbox(Mutex<InboxState>);

impl AudioInbox {
    pub fn new() -> Self {
        Self(Mutex::new(InboxState {
            commands: VecDeque::with_capacity(LOGICAL_INSTANCES + 2),
            starts: 0,
            resets: 0,
            publisher: SourcePublisher::new(),
            scene: None,
        }))
    }

    pub fn start(&self, request: CueSubmission) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        let failure = if request.execution.lease.cancelled() {
            Some(CueFailure::Cancelled)
        } else if state.starts == LOGICAL_INSTANCES {
            Some(CueFailure::QueueFull)
        } else {
            None
        };
        if let Some(reason) = failure {
            drop(state);
            request.reject(reason);
        } else {
            state.starts += 1;
            state
                .commands
                .push_back(CueCommand::Start(Box::new(request)));
        }
    }

    pub fn set_sources(&self, sources: Vec<DesiredSource>, cancelled: bool) -> u64 {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        let (scene, rejected) = state.publisher.reconcile(sources, cancelled);
        state.scene = Some(scene);
        rejected
    }

    pub fn stop_source(&self, key: SourceKey) {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .publisher
            .stop_source(key);
    }

    pub fn stop_scope(&self, scope: AudioScope, cancellation: &CueCancellation) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        // The same lock orders fresh starts after the reset. Leases acquired
        // before cancellation are refused even if their enqueue races this call.
        cancellation.cancel_scope(scope);
        state.publisher.stop_scope(scope);
        let mut cancelled = Vec::new();
        let mut commands = std::mem::take(&mut state.commands);
        while let Some(command) = commands.pop_front() {
            match command {
                CueCommand::Start(request) if request.scope == scope => {
                    state.starts -= 1;
                    cancelled.push(request);
                }
                CueCommand::StopScope(previous) if previous == scope => state.resets -= 1,
                command => state.commands.push_back(command),
            }
        }
        // At most one reset for each of the two scopes; starts have their own
        // budget, so a full start queue cannot prevent a reset.
        state.commands.push_back(CueCommand::StopScope(scope));
        state.resets += 1;
        drop(state);
        for request in cancelled {
            request.reject(CueFailure::Cancelled);
        }
    }

    pub fn cancel_sources(&self) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        state.publisher.cancel();
        state.scene = None;
    }

    pub fn take(&self, limit: usize) -> (Vec<CueCommand>, Option<SourceScene>) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        let mut commands = Vec::with_capacity(limit.min(state.commands.len()));
        for _ in 0..limit {
            let Some(command) = state.commands.pop_front() else {
                break;
            };
            match &command {
                CueCommand::Start(_) => state.starts -= 1,
                CueCommand::StopScope(_) => state.resets -= 1,
            }
            commands.push(command);
        }
        // A scene can contain sources published after a queued scope reset.
        // Deliver it only after those reset commands have been delivered.
        let scene = (state.resets == 0).then(|| state.scene.take()).flatten();
        (commands, scene)
    }
}
