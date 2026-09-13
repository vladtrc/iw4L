use core::fmt;

const END: u16 = 0xffff;
const COMMENT: u16 = 0xfffe;
const CTAB: u32 = 0x4241_5443;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderKind {
    Pixel,
    Vertex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShaderVersion {
    pub kind: ShaderKind,
    pub major: u8,
    pub minor: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderError {
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
    MalformedCtab,
}

impl fmt::Display for ShaderError {
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
            Self::MalformedCtab => f.write_str("malformed CTAB comment"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Shader<'a> {
    bytes: &'a [u8],
    version: ShaderVersion,
}

impl<'a> Shader<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ShaderError> {
        if bytes.is_empty() {
            return Err(ShaderError::Empty);
        }
        if !bytes.len().is_multiple_of(4) {
            return Err(ShaderError::UnalignedBytes(bytes.len()));
        }
        let version_word = word(bytes, 0).ok_or(ShaderError::Empty)?;
        let kind = match version_word >> 16 {
            0xffff => ShaderKind::Pixel,
            0xfffe => ShaderKind::Vertex,
            _ => return Err(ShaderError::UnsupportedVersion(version_word)),
        };
        let version = ShaderVersion {
            kind,
            major: ((version_word >> 8) & 0xff) as u8,
            minor: (version_word & 0xff) as u8,
        };
        if version.major != 3 || version.minor != 0 {
            return Err(ShaderError::UnsupportedVersion(version_word));
        }

        let words = bytes.len() / 4;
        let mut at = 1;
        loop {
            let token = word(bytes, at).ok_or(ShaderError::MissingEnd)?;
            let opcode = token as u16;
            if opcode == END {
                let trailing = words - at - 1;
                return if trailing == 0 {
                    Ok(Self { bytes, version })
                } else {
                    Err(ShaderError::TrailingWords(trailing))
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
                return Err(ShaderError::Truncated {
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

    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn word_len(self) -> usize {
        self.bytes.len() / 4
    }

    pub fn instructions(self) -> Instructions<'a> {
        Instructions {
            bytes: self.bytes,
            at: 1,
            done: false,
        }
    }

    pub fn ctab(self) -> Result<Option<ConstantTable<'a>>, ShaderError> {
        for instruction in self.instructions() {
            let instruction = instruction?;
            let Some(comment) = instruction.comment_data() else {
                continue;
            };
            for offset in (0..comment.len()).step_by(4) {
                if word(comment, offset / 4) == Some(CTAB) {
                    return ConstantTable::parse(&comment[offset + 4..]).map(Some);
                }
            }
        }
        Ok(None)
    }

    pub fn disassembly(self) -> Disassembly<'a> {
        Disassembly { shader: self }
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
    type Item = Result<Instruction<'a>, ShaderError>;

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
                return Some(Err(ShaderError::Truncated {
                    at_word: self.at,
                    needed: count + 1,
                    have: 0,
                }));
            }
        };
        let operands = match self.bytes.get((self.at + 1) * 4..next * 4) {
            Some(operands) => operands,
            None => {
                return Some(Err(ShaderError::Truncated {
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

    pub fn comment_data(self) -> Option<&'a [u8]> {
        (self.opcode().0 == COMMENT).then_some(self.operands)
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

    const fn source_only(self) -> bool {
        matches!(
            self.0,
            0x19 | 0x1a | 0x1b | 0x1e | 0x28 | 0x29 | 0x2d | 0x26 | 0x60
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ConstantTable<'a> {
    bytes: &'a [u8],
    constant_count: u32,
    constant_info_offset: u32,
}

impl<'a> ConstantTable<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, ShaderError> {
        if bytes.len() < 28 || word(bytes, 0) != Some(28) {
            return Err(ShaderError::MalformedCtab);
        }
        let constant_count = word(bytes, 3).ok_or(ShaderError::MalformedCtab)?;
        let constant_info_offset = word(bytes, 4).ok_or(ShaderError::MalformedCtab)?;
        let entries_end = (constant_info_offset as usize)
            .checked_add((constant_count as usize).saturating_mul(20))
            .ok_or(ShaderError::MalformedCtab)?;
        if entries_end > bytes.len() {
            return Err(ShaderError::MalformedCtab);
        }
        Ok(Self {
            bytes,
            constant_count,
            constant_info_offset,
        })
    }

    pub const fn len(self) -> usize {
        self.constant_count as usize
    }

    pub const fn is_empty(self) -> bool {
        self.constant_count == 0
    }

    pub fn constant(self, index: usize) -> Result<Constant<'a>, ShaderError> {
        if index >= self.len() {
            return Err(ShaderError::MalformedCtab);
        }
        let at = self.constant_info_offset as usize + index * 20;
        let name_offset = word_at(self.bytes, at)?;
        let register_set = u16_at(self.bytes, at + 4)?;
        let register_index = u16_at(self.bytes, at + 6)?;
        let register_count = u16_at(self.bytes, at + 8)?;
        let name = c_string_at(self.bytes, name_offset as usize)?;
        Ok(Constant {
            name,
            register_set,
            register_index,
            register_count,
        })
    }

    pub fn register_name(
        self,
        register_set: u16,
        register: u16,
    ) -> Result<Option<&'a [u8]>, ShaderError> {
        for index in 0..self.len() {
            let constant = self.constant(index)?;
            if constant.register_set == register_set
                && register >= constant.register_index
                && register
                    < constant
                        .register_index
                        .saturating_add(constant.register_count)
            {
                return Ok(Some(constant.name));
            }
        }
        Ok(None)
    }
}

fn word_at(bytes: &[u8], at: usize) -> Result<u32, ShaderError> {
    let bytes: [u8; 4] = bytes
        .get(at..at + 4)
        .ok_or(ShaderError::MalformedCtab)?
        .try_into()
        .map_err(|_| ShaderError::MalformedCtab)?;
    Ok(u32::from_le_bytes(bytes))
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, ShaderError> {
    let bytes: [u8; 2] = bytes
        .get(at..at + 2)
        .ok_or(ShaderError::MalformedCtab)?
        .try_into()
        .map_err(|_| ShaderError::MalformedCtab)?;
    Ok(u16::from_le_bytes(bytes))
}

fn c_string_at(bytes: &[u8], at: usize) -> Result<&[u8], ShaderError> {
    let bytes = bytes.get(at..).ok_or(ShaderError::MalformedCtab)?;
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .ok_or(ShaderError::MalformedCtab)?;
    Ok(&bytes[..end])
}

#[derive(Clone, Copy, Debug)]
pub struct Constant<'a> {
    pub name: &'a [u8],
    pub register_set: u16,
    pub register_index: u16,
    pub register_count: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct Disassembly<'a> {
    shader: Shader<'a>,
}

impl fmt::Display for Disassembly<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let version = self.shader.version();
        let kind = match version.kind {
            ShaderKind::Pixel => "ps",
            ShaderKind::Vertex => "vs",
        };
        writeln!(f, "{kind}_{}_{}", version.major, version.minor)?;
        let ctab = self.shader.ctab().map_err(|_| fmt::Error)?;
        if let Some(ctab) = ctab {
            writeln!(f, "; {} named constants/samplers", ctab.len())?;
        } else {
            writeln!(f, "; 0 named constants/samplers")?;
        }
        for instruction in self.shader.instructions() {
            let instruction = instruction.map_err(|_| fmt::Error)?;
            if instruction.comment_data().is_some() {
                continue;
            }
            format_instruction(f, instruction, ctab)?;
        }
        Ok(())
    }
}

fn format_instruction(
    f: &mut fmt::Formatter<'_>,
    instruction: Instruction<'_>,
    ctab: Option<ConstantTable<'_>>,
) -> fmt::Result {
    let opcode = instruction.opcode();
    let name = opcode.name().unwrap_or("unknown");
    if opcode.0 == END {
        return writeln!(f, "end");
    }
    if opcode.0 == 0x51 && instruction.operand_len() == 5 {
        write!(f, "  def ")?;
        format_register(f, instruction.operand(0).ok_or(fmt::Error)?, ctab, true)?;
        for index in 1..5 {
            let value = f32::from_bits(instruction.operand(index).ok_or(fmt::Error)?);
            f.write_str(", ")?;
            format_float_g6(f, value)?;
        }
        return writeln!(f);
    }
    if opcode.0 == 0x1f {
        write!(f, "  dcl ")?;
        if instruction.operand_len() >= 2 {
            format_register(f, instruction.operand(1).ok_or(fmt::Error)?, ctab, true)?;
            return writeln!(
                f,
                "   ; usage {:#x}",
                instruction.operand(0).ok_or(fmt::Error)?
            );
        }
        if instruction.operand_len() == 1 {
            format_register(f, instruction.operand(0).ok_or(fmt::Error)?, ctab, true)?;
        }
        return writeln!(f);
    }
    let comparison = match (opcode.0, (instruction.token >> 16) & 7) {
        (0x29 | 0x2d, 1) => "_gt",
        (0x29 | 0x2d, 2) => "_eq",
        (0x29 | 0x2d, 3) => "_ge",
        (0x29 | 0x2d, 4) => "_lt",
        (0x29 | 0x2d, 5) => "_ne",
        (0x29 | 0x2d, 6) => "_le",
        _ => "",
    };
    write!(f, "  {name}{comparison}")?;
    let has_destination = !opcode.source_only() && instruction.operand_len() > 0;
    if has_destination {
        let dest = instruction.operand(0).ok_or(fmt::Error)?;
        format_destination_modifiers(f, dest)?;
        write!(f, " ")?;
        format_register(f, dest, ctab, true)?;
    }
    let start = usize::from(has_destination);
    if !has_destination && instruction.operand_len() > 0 {
        write!(f, " ")?;
    } else if has_destination && instruction.operand_len() > start {
        write!(f, ", ")?;
    }
    for index in start..instruction.operand_len() {
        if index > start {
            write!(f, ", ")?;
        }
        let operand = instruction.operand(index).ok_or(fmt::Error)?;
        format_source(f, operand, ctab)?;
    }
    writeln!(f)
}

fn format_destination_modifiers(f: &mut fmt::Formatter<'_>, token: u32) -> fmt::Result {
    let modifiers = (token >> 20) & 0x0f;
    if modifiers & 1 != 0 {
        f.write_str("_sat")?;
    }
    if modifiers & 2 != 0 {
        f.write_str("_pp")?;
    }
    if modifiers & 4 != 0 {
        f.write_str("_centroid")?;
    }
    Ok(())
}

fn format_register(
    f: &mut fmt::Formatter<'_>,
    token: u32,
    ctab: Option<ConstantTable<'_>>,
    destination: bool,
) -> fmt::Result {
    let register = (token & 0x7ff) as u16;
    let register_type = (((token & 0x7000_0000) >> 28) | ((token & 0x0000_1800) >> 8)) as u16;
    let name = match register_type {
        0 => "r",
        1 => "v",
        2 | 11 | 12 | 13 => "c",
        3 => "t",
        4 => "oPos",
        5 => "oD",
        6 => "o",
        7 => "i",
        8 => "oC",
        9 => "oDepth",
        10 => "s",
        14 => "b",
        15 => "aL",
        16 => "rf16",
        17 => "misc",
        18 => "label",
        19 => "p",
        _ => "?",
    };
    let no_number = matches!(register_type, 4 | 9 | 15);
    write!(f, "{name}")?;
    if !no_number {
        write!(f, "{register}")?;
    }
    let ctab_set = match name {
        "c" => Some(2),
        "s" => Some(3),
        "b" => Some(0),
        "i" => Some(1),
        _ => None,
    };
    if let (Some(ctab), Some(set)) = (ctab, ctab_set)
        && let Ok(Some(constant)) = ctab.register_name(set, register)
        && let Ok(constant) = core::str::from_utf8(constant)
    {
        write!(f, "[{constant}]")?;
    }
    if destination {
        let mask = (token >> 16) & 0x0f;
        if mask != 0x0f {
            f.write_str(".")?;
            for (bit, component) in [(1, 'x'), (2, 'y'), (4, 'z'), (8, 'w')] {
                if mask & bit != 0 {
                    write!(f, "{component}")?;
                }
            }
        }
    } else {
        let swizzle = (token >> 16) & 0xff;
        if swizzle != 0xe4 {
            let components = ['x', 'y', 'z', 'w'];
            let first = components[(swizzle & 3) as usize];
            let splat = (1..4).all(|index| ((swizzle >> (index * 2)) & 3) == (swizzle & 3));
            write!(f, ".{first}")?;
            if !splat {
                for index in 1..4 {
                    write!(f, "{}", components[((swizzle >> (index * 2)) & 3) as usize])?;
                }
            }
        }
    }
    Ok(())
}

fn format_source(
    f: &mut fmt::Formatter<'_>,
    token: u32,
    ctab: Option<ConstantTable<'_>>,
) -> fmt::Result {
    match (token >> 24) & 0x0f {
        0 => format_register(f, token, ctab, false),
        1 => {
            f.write_str("-")?;
            format_register(f, token, ctab, false)
        }
        3 => {
            f.write_str("|")?;
            format_register(f, token, ctab, false)?;
            f.write_str("|")
        }
        4 => {
            f.write_str("-|")?;
            format_register(f, token, ctab, false)?;
            f.write_str("|")
        }

        _ => format_register(f, token, ctab, false),
    }
}

fn format_float_g6(f: &mut fmt::Formatter<'_>, value: f32) -> fmt::Result {
    if value == 0.0 {
        return f.write_str("0");
    }
    if !value.is_finite() {
        return write!(f, "{value}");
    }
    let negative = value.is_sign_negative();
    let absolute = value.abs();
    let mut precision = 0usize;
    if absolute >= 1.0 {
        let mut whole = absolute as u64;
        let mut digits = 1usize;
        while whole >= 10 {
            whole /= 10;
            digits += 1;
        }
        precision = 6usize.saturating_sub(digits);
    } else {
        let mut scaled = absolute;
        while scaled < 1.0 {
            scaled *= 10.0;
            precision += 1;
        }
        precision += 5;
    }
    let mut scale = 1u64;
    for _ in 0..precision {
        scale = scale.saturating_mul(10);
    }
    let mut rounded = (absolute * scale as f32 + 0.5) as u64;
    while precision > 0 && rounded.is_multiple_of(10) {
        rounded /= 10;
        scale /= 10;
        precision -= 1;
    }
    if negative {
        f.write_str("-")?;
    }
    if precision == 0 {
        return write!(f, "{rounded}");
    }
    if rounded < scale {
        f.write_str("0.")?;
        let mut padding = scale / 10;
        while padding > rounded && padding > 1 {
            f.write_str("0")?;
            padding /= 10;
        }
        return write!(f, "{rounded}");
    }
    write!(f, "{}.", rounded / scale)?;
    let fraction = rounded % scale;
    let mut padding = scale / 10;
    while padding > fraction && padding > 1 {
        f.write_str("0")?;
        padding /= 10;
    }
    write!(f, "{fraction}")
}
