use core::fmt;

const END: u16 = 0xffff;
const COMMENT: u16 = 0xfffe;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderStage {
    Pixel,
    Vertex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShaderVersion {
    pub stage: ShaderStage,
    pub major: u8,
    pub minor: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenError {
    Empty,
    UnalignedBytes(usize),
    UnsupportedVersion(u32),
    Truncated {
        at_word: usize,
        needed: usize,
        have: usize,
    },
    MissingEnd,
    TrailingWords(usize),
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("shader has no version token"),
            Self::UnalignedBytes(len) => write!(f, "shader length {len} is not DWORD-aligned"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported shader version {version:#010x}")
            }
            Self::Truncated {
                at_word,
                needed,
                have,
            } => write!(
                f,
                "token at word {at_word} needs {needed} words, only {have} remain"
            ),
            Self::MissingEnd => f.write_str("shader has no END token"),
            Self::TrailingWords(words) => write!(f, "END is followed by {words} words"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TokenStream<'a> {
    bytes: &'a [u8],
    version: ShaderVersion,
}

impl<'a> TokenStream<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, TokenError> {
        if bytes.is_empty() {
            return Err(TokenError::Empty);
        }
        if !bytes.len().is_multiple_of(4) {
            return Err(TokenError::UnalignedBytes(bytes.len()));
        }
        let version_word = word(bytes, 0).ok_or(TokenError::Empty)?;
        let stage = match version_word >> 16 {
            0xffff => ShaderStage::Pixel,
            0xfffe => ShaderStage::Vertex,
            _ => return Err(TokenError::UnsupportedVersion(version_word)),
        };
        let version = ShaderVersion {
            stage,
            major: ((version_word >> 8) & 0xff) as u8,
            minor: (version_word & 0xff) as u8,
        };
        if version.major != 3 || version.minor != 0 {
            return Err(TokenError::UnsupportedVersion(version_word));
        }

        let words = bytes.len() / 4;
        let mut at = 1;
        loop {
            let token = word(bytes, at).ok_or(TokenError::MissingEnd)?;
            let opcode = token as u16;
            if opcode == END {
                let trailing = words - at - 1;
                return if trailing == 0 {
                    Ok(Self { bytes, version })
                } else {
                    Err(TokenError::TrailingWords(trailing))
                };
            }
            let operand_words = if opcode == COMMENT {
                ((token >> 16) & 0x7fff) as usize
            } else {
                ((token >> 24) & 0x0f) as usize
            };
            let needed = operand_words + 1;
            let have = words - at;
            if have < needed {
                return Err(TokenError::Truncated {
                    at_word: at,
                    needed,
                    have,
                });
            }
            at += needed;
        }
    }

    pub const fn version(self) -> ShaderVersion {
        self.version
    }

    pub fn instructions(self) -> Instructions<'a> {
        Instructions {
            bytes: self.bytes,
            at: 1,
            done: false,
        }
    }
}

fn word(bytes: &[u8], index: usize) -> Option<u32> {
    let at = index.checked_mul(4)?;
    let bytes: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

#[derive(Clone, Copy, Debug)]
pub struct Instructions<'a> {
    bytes: &'a [u8],
    at: usize,
    done: bool,
}

impl<'a> Iterator for Instructions<'a> {
    type Item = Result<Instruction<'a>, TokenError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let token = word(self.bytes, self.at)?;
        let opcode = token as u16;
        if opcode == END {
            self.done = true;
            return Some(Ok(Instruction {
                token,
                operands: &[],
                at_word: self.at,
            }));
        }
        let count = if opcode == COMMENT {
            ((token >> 16) & 0x7fff) as usize
        } else {
            ((token >> 24) & 0x0f) as usize
        };
        let next = match self.at.checked_add(count + 1) {
            Some(next) => next,
            None => {
                return Some(Err(TokenError::Truncated {
                    at_word: self.at,
                    needed: count + 1,
                    have: 0,
                }));
            }
        };
        let operands = match self.bytes.get((self.at + 1) * 4..next * 4) {
            Some(operands) => operands,
            None => {
                return Some(Err(TokenError::Truncated {
                    at_word: self.at,
                    needed: count + 1,
                    have: self.bytes.len() / 4 - self.at,
                }));
            }
        };
        let instruction = Instruction {
            token,
            operands,
            at_word: self.at,
        };
        self.at = next;
        Some(Ok(instruction))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Instruction<'a> {
    token: u32,
    operands: &'a [u8],
    at_word: usize,
}

impl<'a> Instruction<'a> {
    pub const fn at_word(self) -> usize {
        self.at_word
    }

    pub const fn opcode(self) -> Opcode {
        Opcode(self.token as u16)
    }

    pub const fn raw_token(self) -> u32 {
        self.token
    }

    pub const fn predicated(self) -> bool {
        self.token & 0x1000_0000 != 0
    }

    pub const fn coissue(self) -> bool {
        self.token & 0x4000_0000 != 0
    }

    pub const fn operand_len(self) -> usize {
        self.operands.len() / 4
    }

    pub fn operand(self, index: usize) -> Option<u32> {
        word(self.operands, index)
    }

    pub fn is_comment(self) -> bool {
        self.opcode().raw() == COMMENT
    }

    pub fn is_end(self) -> bool {
        self.opcode().raw() == END
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opcode(u16);

impl Opcode {
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            0x00 => "nop",
            0x01 => "mov",
            0x02 => "add",
            0x03 => "sub",
            0x04 => "mad",
            0x05 => "mul",
            0x06 => "rcp",
            0x07 => "rsq",
            0x08 => "dp3",
            0x09 => "dp4",
            0x0a => "min",
            0x0b => "max",
            0x0c => "slt",
            0x0d => "sge",
            0x0e => "exp",
            0x0f => "log",
            0x10 => "lit",
            0x11 => "dst",
            0x12 => "lrp",
            0x13 => "frc",
            0x14 => "m4x4",
            0x15 => "m4x3",
            0x16 => "m3x4",
            0x17 => "m3x3",
            0x18 => "m3x2",
            0x19 => "call",
            0x1a => "callnz",
            0x1b => "loop",
            0x1c => "ret",
            0x1d => "endloop",
            0x1e => "label",
            0x1f => "dcl",
            0x20 => "pow",
            0x21 => "crs",
            0x22 => "sgn",
            0x23 => "abs",
            0x24 => "nrm",
            0x25 => "sincos",
            0x26 => "rep",
            0x27 => "endrep",
            0x28 => "if",
            0x29 => "ifc",
            0x2a => "else",
            0x2b => "endif",
            0x2c => "break",
            0x2d => "breakc",
            0x2e => "mova",
            0x2f => "defb",
            0x30 => "defi",
            0x40 => "texcoord",
            0x41 => "texkill",
            0x42 => "texld",
            0x4e => "expp",
            0x4f => "logp",
            0x50 => "cnd",
            0x51 => "def",
            0x58 => "cmp",
            0x59 => "bem",
            0x5a => "dp2add",
            0x5b => "dsx",
            0x5c => "dsy",
            0x5d => "texldd",
            0x5e => "setp",
            0x5f => "texldl",
            0x60 => "breakp",
            END => "end",
            COMMENT => "comment",
            _ => return None,
        })
    }
}
