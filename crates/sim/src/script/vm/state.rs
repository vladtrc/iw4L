use bevy_ecs::prelude::Component;
use std::sync::Arc;

use crate::script::Value;

#[derive(Clone, Debug)]
pub(crate) struct Frame {
    pub(crate) function: usize,
    pub(crate) pc: usize,
    pub(crate) stack_base: usize,
    pub(crate) receiver: Value,
    pub(crate) locals: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ThreadState {
    Runnable,
    Queued,
    Awaiting,
    Complete,
}

#[derive(Component, Clone, Debug)]
pub(crate) struct Thread {
    pub(crate) serial: u64,
    pub(crate) frames: Vec<Frame>,
    pub(crate) stack: Vec<Value>,
    pub(crate) state: ThreadState,
}

#[derive(Clone, Debug)]
pub(crate) enum WaiterKind {
    Endon { frame: usize },
    Waittill { outputs: Vec<u32> },
    Match { values: Vec<Value> },
}

#[derive(Clone, Debug)]
pub(crate) struct Waiter {
    pub(crate) receiver: Value,
    pub(crate) name: Arc<str>,
    pub(crate) thread: u64,
    pub(crate) kind: WaiterKind,
}
