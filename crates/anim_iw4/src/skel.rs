#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DuplicatePart {
    pub destination: u8,
    pub source: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skel {
    pub duplicate_part: DuplicatePart,
}
