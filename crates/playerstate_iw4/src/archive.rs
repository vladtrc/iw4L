use crate::PlayerState;

#[derive(Clone, Copy, Debug)]
pub struct RemappedTimer {
    pub offset: usize,

    pub retail_name: &'static str,

    pub unconditional: bool,
}

pub const REMAPPED_PS_TIMERS: &[RemappedTimer] = &[
    RemappedTimer {
        offset: 0x000,
        retail_name: "commandTime",
        unconditional: false,
    },
    RemappedTimer {
        offset: 0x008,
        retail_name: "pm_time",
        unconditional: false,
    },
    RemappedTimer {
        offset: 0x050,
        retail_name: "foliageSoundTime",
        unconditional: false,
    },
    RemappedTimer {
        offset: 0x07c,
        retail_name: "jumpTime",
        unconditional: false,
    },
    RemappedTimer {
        offset: 0x120,
        retail_name: "viewHeightLerpTime",
        unconditional: false,
    },
    RemappedTimer {
        offset: 0x490,
        retail_name: "shellshockTime",
        unconditional: false,
    },
    RemappedTimer {
        offset: 0x838,
        retail_name: "deltaTime",
        unconditional: true,
    },
];

pub fn remapped_timer(offset: usize) -> Option<&'static RemappedTimer> {
    let mut i = 0;
    while i < REMAPPED_PS_TIMERS.len() {
        if REMAPPED_PS_TIMERS[i].offset == offset {
            return Some(&REMAPPED_PS_TIMERS[i]);
        }
        i += 1;
    }
    None
}

impl PlayerState {
    pub const fn is_live_frame(&self) -> bool {
        self.delta_time == 0
    }
}
