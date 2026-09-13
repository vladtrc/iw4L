use movement_iw4::{
    CreateAnimsMantleRootDelta, FlatMantleAnimLength, MANTLE_XANIM_NAMES, MANTLE_XANIM_NAMES_FR,
    MANTLE_XANIM_TREE_SIZE, MantleRootDelta, MantleXAnimLength, mantle_trans_over_anim,
    mantle_trans_up_anim,
};
use xmodel_runtime::AnimClip;

#[derive(Clone, Debug)]
struct Leaf {
    length_msec: i32,
    clip: Option<AnimClip>,
}

#[derive(Clone, Debug)]
pub struct MantleXAnimBind {
    slow: [Leaf; MANTLE_XANIM_TREE_SIZE],
    fast: [Leaf; MANTLE_XANIM_TREE_SIZE],
}

impl Default for MantleXAnimBind {
    fn default() -> Self {
        Self {
            slow: core::array::from_fn(|_| Leaf {
                length_msec: 0,
                clip: None,
            }),
            fast: core::array::from_fn(|_| Leaf {
                length_msec: 0,
                clip: None,
            }),
        }
    }
}

impl MantleXAnimBind {
    #[must_use]
    pub fn clip_name(fast: bool, index: usize) -> Option<&'static str> {
        let names = if fast {
            MANTLE_XANIM_NAMES_FR.as_slice()
        } else {
            MANTLE_XANIM_NAMES.as_slice()
        };
        names.get(index).copied()
    }

    #[must_use]
    pub fn up_anim(trans_index: i32) -> i32 {
        mantle_trans_up_anim(trans_index)
    }

    #[must_use]
    pub fn over_anim(trans_index: i32) -> i32 {
        mantle_trans_over_anim(trans_index)
    }

    pub fn from_clips(mut get: impl FnMut(bool, usize) -> Option<AnimClip>) -> Self {
        let fill = |fast: bool, get: &mut dyn FnMut(bool, usize) -> Option<AnimClip>| {
            core::array::from_fn(|i| match get(fast, i) {
                Some(clip) => Leaf {
                    length_msec: clip.length_msec().max(1),
                    clip: Some(clip),
                },
                None => Leaf {
                    length_msec: 0,
                    clip: None,
                },
            })
        };
        Self {
            slow: fill(false, &mut get),
            fast: fill(true, &mut get),
        }
    }

    fn leaf(&self, fast: bool, anim: i32) -> Option<&Leaf> {
        let i = usize::try_from(anim).ok()?;
        let row = if fast { &self.fast } else { &self.slow };
        row.get(i)
    }
}

impl MantleXAnimLength for MantleXAnimBind {
    fn length_msec(&self, fast_mantle: bool, anim_index: i32) -> i32 {
        if let Some(leaf) = self.leaf(fast_mantle, anim_index) {
            if leaf.length_msec > 0 {
                return leaf.length_msec;
            }
        }
        FlatMantleAnimLength::default().length_msec(fast_mantle, anim_index)
    }
}

impl MantleRootDelta for MantleXAnimBind {
    fn abs_delta(&self, fast_mantle: bool, anim_index: i32, frac: f32) -> [f32; 3] {
        if let Some(leaf) = self.leaf(fast_mantle, anim_index) {
            if let Some(clip) = leaf.clip.as_ref() {
                if clip.has_delta() {
                    return clip.abs_delta_trans(frac);
                }
                return [0.0; 3];
            }
        }
        CreateAnimsMantleRootDelta.abs_delta(fast_mantle, anim_index, frac)
    }
}
