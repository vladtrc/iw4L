mod convolution;
mod fft;
mod presets;

use convolution::{DelayedStage, Kernel, Stage};
use fft::Fft;
use presets::PRESETS;
use std::sync::OnceLock;

const RATE: usize = 48_000;
const QUANTUM: usize = 128;
const FADE_FRAMES: usize = RATE * 128 / 1000;
type Block = [[f32; 2]; QUANTUM];

struct RoomKernel {
    head: Kernel,
    middle: Kernel,
    tail: Kernel,
}

struct RoomBank {
    rooms: Box<[RoomKernel]>,
    head_fft: Fft<256>,
    middle_fft: Fft<2048>,
    tail_fft: Fft<8192>,
}

impl RoomBank {
    fn prepare() -> Self {
        let head_fft = Fft::default();
        let middle_fft = Fft::default();
        let tail_fft = Fft::default();
        let rooms = PRESETS
            .iter()
            .enumerate()
            .map(|(index, preset)| {
                let impulse = std::array::from_fn(|channel| preset.impulse(index, channel));
                RoomKernel {
                    head: Kernel::new::<128, 256>(&impulse, 0, 1024, &head_fft),
                    middle: Kernel::new::<1024, 2048>(&impulse, 1024, 8192, &middle_fft),
                    tail: Kernel::new::<4096, 8192>(&impulse, 8192, impulse[0].len(), &tail_fft),
                }
            })
            .collect();
        Self {
            rooms,
            head_fft,
            middle_fft,
            tail_fft,
        }
    }
}

struct RoomRender {
    head: Stage<128, 256>,
    middle: DelayedStage<1024, 2048>,
    tail: DelayedStage<4096, 8192>,
}

impl RoomRender {
    fn new(bank: &RoomBank) -> Self {
        Self {
            head: Stage::new(bank.rooms.iter().map(|r| r.head.parts()).max().unwrap()),
            middle: DelayedStage::new(
                bank.rooms.iter().map(|r| r.middle.parts()).max().unwrap(),
                1024,
            ),
            tail: DelayedStage::new(
                bank.rooms.iter().map(|r| r.tail.parts()).max().unwrap(),
                8192,
            ),
        }
    }

    fn clear(&mut self) {
        self.head.clear();
        self.middle.clear();
        self.tail.clear();
    }

    fn process(&mut self, input: &Block, index: usize, bank: &RoomBank) -> Block {
        let room = &bank.rooms[index];
        let mut output = self.head.process(input, &room.head, &bank.head_fft);
        let middle = self.middle.process(input, &room.middle, &bank.middle_fft);
        let tail = self.tail.process(input, &room.tail, &bank.tail_fft);
        for i in 0..QUANTUM {
            for side in 0..2 {
                output[i][side] += middle[i][side] + tail[i][side];
            }
        }
        output
    }
}

pub(crate) struct RoomProcessor {
    bank: &'static RoomBank,
    renders: [RoomRender; 2],
    rooms: [usize; 2],
    active: usize,
    pending: usize,
    fade: Option<usize>,
    started: bool,
}

impl Default for RoomProcessor {
    fn default() -> Self {
        // Preparation and allocation precede device startup; the callback only borrows this bank.
        static BANK: OnceLock<RoomBank> = OnceLock::new();
        let bank = BANK.get_or_init(RoomBank::prepare);
        Self {
            bank,
            renders: std::array::from_fn(|_| RoomRender::new(bank)),
            rooms: [0; 2],
            active: 0,
            pending: 0,
            fade: None,
            started: false,
        }
    }
}

impl RoomProcessor {
    pub(crate) fn clear(&mut self) {
        for render in &mut self.renders {
            render.clear();
        }
        self.rooms = [0; 2];
        self.active = 0;
        self.pending = 0;
        self.fade = None;
        self.started = false;
    }

    pub(crate) fn room_index(name: &str) -> Option<u8> {
        PRESETS
            .iter()
            .position(|preset| preset.name.eq_ignore_ascii_case(name))
            .map(|index| index as u8)
    }

    pub(crate) fn set_room(&mut self, index: u8) {
        self.pending = if usize::from(index) < PRESETS.len() {
            usize::from(index)
        } else {
            0
        };
        if !self.started {
            self.rooms[self.active] = self.pending;
        }
    }

    pub(crate) fn process_block(&mut self, input: &Block) -> Block {
        self.started = true;
        if self.fade.is_none() && self.rooms[self.active] != self.pending {
            let next = 1 - self.active;
            self.renders[next].clear();
            self.rooms[next] = self.pending;
            self.fade = Some(0);
        }
        let mut output =
            self.renders[self.active].process(input, self.rooms[self.active], self.bank);
        if let Some(frame) = self.fade {
            let next = 1 - self.active;
            let incoming = self.renders[next].process(input, self.rooms[next], self.bank);
            for i in 0..QUANTUM {
                let fraction = ((frame + i) as f32 / FADE_FRAMES as f32).min(1.0);
                for side in 0..2 {
                    output[i][side] += (incoming[i][side] - output[i][side]) * fraction;
                }
            }
            if frame + QUANTUM >= FADE_FRAMES {
                self.active = next;
                self.fade = None;
            } else {
                self.fade = Some(frame + QUANTUM);
            }
        }
        output
    }
}
