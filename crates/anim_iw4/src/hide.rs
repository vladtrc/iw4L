use crate::part_bits::PartBits;

pub fn set_hide_part_bit(words: &mut [u32; 6], bone: usize) {
    if bone >= PartBits::CAPACITY {
        return;
    }
    words[bone >> 5] |= 0x8000_0000u32 >> (bone & 31);
}

#[must_use]
pub fn hide_part_bit(words: &[u32; 6], bone: usize) -> bool {
    bone < PartBits::CAPACITY && words[bone >> 5] & (0x8000_0000u32 >> (bone & 31)) != 0
}

pub fn or_shift_part_bits(dst: &mut [u32; 6], src: [u32; 6], bone_base: u32) {
    let word_shift = (bone_base >> 5) as usize;
    let bit_shift = bone_base & 31;
    if bit_shift == 0 {
        for i in 0..6 {
            if i >= word_shift {
                dst[i] |= src[i - word_shift];
            }
        }
        return;
    }
    let inv = 32 - bit_shift;
    for i in 0..6 {
        let low = if i >= word_shift {
            src[i - word_shift] >> bit_shift
        } else {
            0
        };
        let high = if i >= word_shift + 1 {
            src[i - word_shift - 1] << inv
        } else {
            0
        };
        dst[i] |= low | high;
    }
}

#[must_use]
pub fn dobj_surface_hidden(part_bits: &[u32; 6], hide: &[u32; 6], bone_base: u32) -> bool {
    let mut lifted = [0u32; 6];
    or_shift_part_bits(&mut lifted, *part_bits, bone_base);
    lifted.iter().zip(hide.iter()).any(|(a, b)| a & b != 0)
}
