use crate::log::Log;

pub type Millis = i32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wake<K: Copy> {
    Notify(K),

    Deadline(Millis),

    Parked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Woke<K: Copy> {
    Notify(K),
    Deadline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event<K: Copy> {
    Woke { id: TaskId, pc: u16, cause: Woke<K> },

    Ended { id: TaskId, pc: u16, by: K },
}

impl<K: Copy> Event<K> {
    pub const fn id(&self) -> TaskId {
        match self {
            Event::Woke { id, .. } | Event::Ended { id, .. } => *id,
        }
    }

    pub const fn pc(&self) -> u16 {
        match self {
            Event::Woke { pc, .. } | Event::Ended { pc, .. } => *pc,
        }
    }
}

const ENDON_CAP: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndOn<K: Copy + Eq> {
    kinds: [Option<K>; ENDON_CAP],
    len: usize,
}

impl<K: Copy + Eq> Default for EndOn<K> {
    fn default() -> Self {
        Self::none()
    }
}

impl<K: Copy + Eq> EndOn<K> {
    pub const CAP: usize = ENDON_CAP;

    pub const fn none() -> Self {
        Self {
            kinds: [None; ENDON_CAP],
            len: 0,
        }
    }

    #[must_use]
    pub fn on(mut self, kind: K) -> Self {
        assert!(self.len < Self::CAP, "more endon kinds than EndOn::CAP");
        self.kinds[self.len] = Some(kind);
        self.len += 1;
        self
    }

    pub fn contains(&self, kind: K) -> bool {
        self.kinds
            .iter()
            .take(self.len)
            .any(|k| k.is_some_and(|k| k == kind))
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Task<K: Copy + Eq> {
    pub id: TaskId,

    pub pc: u16,
    pub wake: Wake<K>,
    pub endon: EndOn<K>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnError {
    Full,

    EndOnAlsoAwaited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResumeError {
    Gone,

    EndOnAlsoAwaited,
}

#[derive(Clone, Copy, Debug)]
pub struct Scheduler<K: Copy + Eq + PartialEq, const N: usize> {
    slots: [Option<Task<K>>; N],
    next_id: u16,
}

impl<K: Copy + Eq + PartialEq, const N: usize> Default for Scheduler<K, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Copy + Eq + PartialEq, const N: usize> Scheduler<K, N> {
    pub const fn new() -> Self {
        Self {
            slots: [None; N],
            next_id: 0,
        }
    }

    pub fn spawn(&mut self, pc: u16, wake: Wake<K>, endon: EndOn<K>) -> Result<TaskId, SpawnError> {
        if let Wake::Notify(k) = wake
            && endon.contains(k)
        {
            return Err(SpawnError::EndOnAlsoAwaited);
        }
        let free = self
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(SpawnError::Full)?;
        let id = TaskId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.slots[free] = Some(Task {
            id,
            pc,
            wake,
            endon,
        });
        Ok(id)
    }

    pub fn resume(&mut self, id: TaskId, pc: u16, wake: Wake<K>) -> Result<(), ResumeError> {
        let task = self.slot_mut(id).ok_or(ResumeError::Gone)?;
        if let Wake::Notify(k) = wake
            && task.endon.contains(k)
        {
            return Err(ResumeError::EndOnAlsoAwaited);
        }
        task.pc = pc;
        task.wake = wake;
        Ok(())
    }

    pub fn add_endon(&mut self, id: TaskId, kind: K) -> Result<(), ResumeError> {
        let task = self.slot_mut(id).ok_or(ResumeError::Gone)?;
        if task.wake == Wake::Notify(kind) {
            return Err(ResumeError::EndOnAlsoAwaited);
        }
        if !task.endon.contains(kind) {
            task.endon = task.endon.on(kind);
        }
        Ok(())
    }

    pub fn finish(&mut self, id: TaskId) {
        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|s| s.is_some_and(|t| t.id == id))
        {
            *slot = None;
        }
    }

    pub fn task(&self, id: TaskId) -> Option<Task<K>> {
        self.slots.iter().flatten().find(|t| t.id == id).copied()
    }

    pub fn is_alive(&self, id: TaskId) -> bool {
        self.task(id).is_some()
    }

    pub fn len(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn advance(&mut self, now_ms: Millis, incoming: &[K]) -> Log<Event<K>, N> {
        let mut out = Log::new();
        for &kind in incoming {
            for slot in self.slots.iter_mut() {
                let Some(task) = slot else { continue };
                if task.endon.contains(kind) {
                    out.push(Event::Ended {
                        id: task.id,
                        pc: task.pc,
                        by: kind,
                    });
                    *slot = None;
                }
            }
            for task in self.slots.iter_mut().flatten() {
                if task.wake == Wake::Notify(kind) {
                    out.push(Event::Woke {
                        id: task.id,
                        pc: task.pc,
                        cause: Woke::Notify(kind),
                    });
                    task.wake = Wake::Parked;
                }
            }
        }
        for task in self.slots.iter_mut().flatten() {
            if let Wake::Deadline(deadline) = task.wake
                && now_ms >= deadline
            {
                out.push(Event::Woke {
                    id: task.id,
                    pc: task.pc,
                    cause: Woke::Deadline,
                });
                task.wake = Wake::Parked;
            }
        }
        out
    }

    fn slot_mut(&mut self, id: TaskId) -> Option<&mut Task<K>> {
        self.slots.iter_mut().flatten().find(|t| t.id == id)
    }
}
