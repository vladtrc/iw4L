use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::bytecode::{Opcode, ShaderStage, TokenError, TokenStream};

pub const SUPPORTED_OPCODE_SURFACE: &[u16] = &[
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x12, 0x13, 0x1f, 0x20, 0x23, 0x24, 0x25, 0x26, 0x27, 0x29, 0x2a, 0x2b, 0x2e, 0x30, 0x41, 0x42,
    0x51, 0x58, 0x5a, 0x5b, 0x5c, 0x5d, 0x5f,
];

pub fn named_sm3_opcode_count() -> usize {
    (0u16..=0x60)
        .filter(|&raw| {
            let name = Opcode::from_raw(raw).name();
            name.is_some() && name != Some("end") && name != Some("comment")
        })
        .count()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpcodeSurfaceCoverage {
    pub supported_opcodes: usize,

    pub named_sm3_opcodes: usize,

    pub programs_measured: usize,

    pub programs_passed: usize,

    pub programs_refused: usize,

    pub programs_decode_failed: usize,

    pub refused_unknown_opcodes: BTreeMap<u16, usize>,

    pub refused_other: BTreeMap<&'static str, usize>,
}

impl OpcodeSurfaceCoverage {
    pub fn surface_only() -> Self {
        Self {
            supported_opcodes: SUPPORTED_OPCODE_SURFACE.len(),
            named_sm3_opcodes: named_sm3_opcode_count(),
            ..Self::default()
        }
    }

    pub fn measure<'a>(programs: impl IntoIterator<Item = &'a Sm3ProgramIr>) -> Self {
        let mut coverage = Self::surface_only();
        for program in programs {
            coverage.programs_measured = coverage.programs_measured.saturating_add(1);
            match validate_supported_opcode_surface(program) {
                Ok(()) => {
                    coverage.programs_passed = coverage.programs_passed.saturating_add(1);
                }
                Err(refusal) => {
                    coverage.programs_refused = coverage.programs_refused.saturating_add(1);
                    coverage.record_refusal(&refusal);
                }
            }
        }
        coverage
    }

    pub fn measure_shaders<'a>(
        programs: impl IntoIterator<Item = (&'a [u8], ShaderStage)>,
    ) -> Self {
        let mut coverage = Self::surface_only();
        for (bytes, stage) in programs {
            coverage.programs_measured = coverage.programs_measured.saturating_add(1);
            let ir = match decode_sm3_program(bytes, stage) {
                Ok(ir) => ir,
                Err(_) => {
                    coverage.programs_decode_failed =
                        coverage.programs_decode_failed.saturating_add(1);
                    continue;
                }
            };
            match validate_supported_opcode_surface(&ir) {
                Ok(()) => {
                    coverage.programs_passed = coverage.programs_passed.saturating_add(1);
                }
                Err(refusal) => {
                    coverage.programs_refused = coverage.programs_refused.saturating_add(1);
                    coverage.record_refusal(&refusal);
                }
            }
        }
        coverage
    }

    fn record_refusal(&mut self, refusal: &Sm3LoweringRefusal) {
        match *refusal {
            Sm3LoweringRefusal::UnknownOpcode { opcode, .. } => {
                *self.refused_unknown_opcodes.entry(opcode).or_insert(0) += 1;
            }
            Sm3LoweringRefusal::Predication { .. } => {
                *self.refused_other.entry("predication").or_insert(0) += 1;
            }
            Sm3LoweringRefusal::Coissue { .. } => {
                *self.refused_other.entry("coissue").or_insert(0) += 1;
            }
            Sm3LoweringRefusal::RelativeAddressing { .. } => {
                *self.refused_other.entry("relative-addressing").or_insert(0) += 1;
            }
            Sm3LoweringRefusal::UnknownRegisterFile { .. } => {
                *self
                    .refused_other
                    .entry("unknown-register-file")
                    .or_insert(0) += 1;
            }
            Sm3LoweringRefusal::UnknownSourceModifier { .. } => {
                *self
                    .refused_other
                    .entry("unknown-source-modifier")
                    .or_insert(0) += 1;
            }
            Sm3LoweringRefusal::UnknownDestinationModifier { .. } => {
                *self
                    .refused_other
                    .entry("unknown-destination-modifier")
                    .or_insert(0) += 1;
            }
        }
    }

    pub fn summary_line(&self) -> String {
        format!(
            "sm3 opcode surface: supported={}/{} named; corpus measured={} passed={} refused={} decode_failed={}; unknown_opcodes={:?}; other={:?}",
            self.supported_opcodes,
            self.named_sm3_opcodes,
            self.programs_measured,
            self.programs_passed,
            self.programs_refused,
            self.programs_decode_failed,
            self.refused_unknown_opcodes,
            self.refused_other,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sm3RegisterFile {
    Temporary,
    Input,
    FloatConstant,
    Texture,

    Address,
    RasterOutput,
    AttributeOutput,
    Output,
    IntegerConstant,
    ColourOutput,
    DepthOutput,
    Sampler,
    BooleanConstant,
    Loop,
    Float16Temporary,
    Misc,
    Label,
    Predicate,
    Unknown(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sm3Register {
    pub file: Sm3RegisterFile,
    pub index: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Sm3RelativeAddress {
    pub register: Sm3Register,
    pub component: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Sm3Destination {
    pub register: Sm3Register,

    pub write_mask: u8,
    pub saturate: bool,
    pub partial_precision: bool,
    pub centroid: bool,

    pub unknown_modifier_bits: u8,
    pub relative: Option<Sm3RelativeAddress>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sm3SourceModifier {
    None,
    Negate,
    Bias,
    BiasNegate,
    Sign,
    SignNegate,
    Complement,
    Double,
    DoubleNegate,
    DivideByZ,
    DivideByW,
    Absolute,
    AbsoluteNegate,
    LogicalNot,
    Unknown(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Sm3Source {
    pub register: Sm3Register,

    pub swizzle: [u8; 4],
    pub modifier: Sm3SourceModifier,
    pub relative: Option<Sm3RelativeAddress>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sm3Opcode {
    Nop,
    Mov,
    Add,
    Sub,
    Mad,
    Mul,
    Rcp,
    Rsq,
    Dp3,
    Dp4,
    Min,
    Max,
    Slt,
    Sge,
    Exp,
    Log,
    Lrp,
    Frc,
    Abs,
    Nrm,
    Sincos,
    Rep,
    EndRep,
    Ifc,
    Else,
    Endif,
    Mova,
    DefI,
    TexKill,
    Pow,
    Dcl,
    Def,
    Cmp,
    Dp2Add,
    Dsx,
    Dsy,
    TexLd,
    TexLdD,
    TexLdL,
    Unknown(u16),
}

pub const TEXLD_PROJECT: u8 = 1;

#[derive(Clone, Debug, PartialEq)]
pub enum Sm3InstructionBody {
    Operation {
        destination: Option<Sm3Destination>,
        sources: Vec<Sm3Source>,
    },
    Declaration {
        usage_token: Option<u32>,
        destination: Sm3Destination,
    },
    FloatDefinition {
        destination: Sm3Destination,
        values: [u32; 4],
    },
    IntegerDefinition {
        destination: Sm3Destination,
        values: [u32; 4],
    },

    Unmodeled {
        operands: Vec<u32>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sm3Instruction {
    pub at_word: usize,
    pub opcode: Sm3Opcode,

    pub controls: u8,
    pub predicated: bool,
    pub coissue: bool,
    pub body: Sm3InstructionBody,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sm3ProgramIr {
    pub stage: ShaderStage,
    pub instructions: Vec<Sm3Instruction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sm3IrError {
    TokenStream(TokenError),
    StageMismatch {
        expected: ShaderStage,
        actual: ShaderStage,
    },
    MissingOperand {
        at_word: usize,
        operand: usize,
    },
    MalformedRelativeAddress {
        at_word: usize,
        operand: usize,
    },
    InvalidDefinition {
        at_word: usize,
        operand_count: usize,
    },
    InvalidDeclaration {
        at_word: usize,
        operand_count: usize,
    },
    InvalidOperandCount {
        at_word: usize,
        opcode: Sm3Opcode,
        expected_sources: usize,
        actual_sources: usize,
    },
}

impl From<TokenError> for Sm3IrError {
    fn from(value: TokenError) -> Self {
        Self::TokenStream(value)
    }
}

pub fn decode_sm3_program(
    bytes: &[u8],
    expected_stage: ShaderStage,
) -> Result<Sm3ProgramIr, Sm3IrError> {
    let stream = TokenStream::parse(bytes)?;
    let actual = stream.version().stage;
    if actual != expected_stage {
        return Err(Sm3IrError::StageMismatch {
            expected: expected_stage,
            actual,
        });
    }

    let mut instructions = Vec::new();
    for instruction in stream.instructions() {
        let instruction = instruction?;
        if instruction.is_comment() || instruction.is_end() {
            continue;
        }
        let at_word = instruction.at_word();
        let opcode = decode_opcode(instruction.opcode().raw());
        let operands = (0..instruction.operand_len())
            .map(|index| {
                instruction
                    .operand(index)
                    .ok_or(Sm3IrError::MissingOperand {
                        at_word,
                        operand: index,
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let body = match opcode {
            Sm3Opcode::Dcl => decode_declaration(at_word, &operands)?,
            Sm3Opcode::Def => decode_definition(at_word, &operands)?,
            Sm3Opcode::DefI => decode_integer_definition(at_word, &operands)?,
            Sm3Opcode::Unknown(_) => Sm3InstructionBody::Unmodeled { operands },
            _ => decode_operation(at_word, opcode, &operands)?,
        };
        instructions.push(Sm3Instruction {
            at_word,
            opcode,
            controls: ((instruction.raw_token() >> 16) & 0xff) as u8,
            predicated: instruction.predicated(),
            coissue: instruction.coissue(),
            body,
        });
    }
    let mut program = Sm3ProgramIr {
        stage: actual,
        instructions,
    };
    if actual == ShaderStage::Vertex {
        remap_vs_addr_file(&mut program);
    }
    Ok(program)
}

fn remap_vs_addr_file(program: &mut Sm3ProgramIr) {
    let remap = |register: Sm3Register| {
        if register.file == Sm3RegisterFile::Texture {
            Sm3Register {
                file: Sm3RegisterFile::Address,
                index: register.index,
            }
        } else {
            register
        }
    };
    for instruction in &mut program.instructions {
        match &mut instruction.body {
            Sm3InstructionBody::Operation {
                destination,
                sources,
            } => {
                if let Some(destination) = destination {
                    destination.register = remap(destination.register);
                    if let Some(relative) = &mut destination.relative {
                        relative.register = remap(relative.register);
                    }
                }
                for source in sources {
                    source.register = remap(source.register);
                    if let Some(relative) = &mut source.relative {
                        relative.register = remap(relative.register);
                    }
                }
            }
            Sm3InstructionBody::Declaration { destination, .. }
            | Sm3InstructionBody::FloatDefinition { destination, .. }
            | Sm3InstructionBody::IntegerDefinition { destination, .. } => {
                destination.register = remap(destination.register);
                if let Some(relative) = &mut destination.relative {
                    relative.register = remap(relative.register);
                }
            }
            Sm3InstructionBody::Unmodeled { .. } => {}
        }
    }
}

fn decode_opcode(raw: u16) -> Sm3Opcode {
    match raw {
        0x00 => Sm3Opcode::Nop,
        0x01 => Sm3Opcode::Mov,
        0x02 => Sm3Opcode::Add,
        0x03 => Sm3Opcode::Sub,
        0x04 => Sm3Opcode::Mad,
        0x05 => Sm3Opcode::Mul,
        0x06 => Sm3Opcode::Rcp,
        0x07 => Sm3Opcode::Rsq,
        0x08 => Sm3Opcode::Dp3,
        0x09 => Sm3Opcode::Dp4,
        0x0a => Sm3Opcode::Min,
        0x0b => Sm3Opcode::Max,
        0x0c => Sm3Opcode::Slt,
        0x0d => Sm3Opcode::Sge,
        0x0e => Sm3Opcode::Exp,
        0x0f => Sm3Opcode::Log,
        0x12 => Sm3Opcode::Lrp,
        0x13 => Sm3Opcode::Frc,
        0x1f => Sm3Opcode::Dcl,
        0x20 => Sm3Opcode::Pow,
        0x23 => Sm3Opcode::Abs,
        0x24 => Sm3Opcode::Nrm,
        0x25 => Sm3Opcode::Sincos,
        0x26 => Sm3Opcode::Rep,
        0x27 => Sm3Opcode::EndRep,
        0x29 => Sm3Opcode::Ifc,
        0x2a => Sm3Opcode::Else,
        0x2b => Sm3Opcode::Endif,
        0x2e => Sm3Opcode::Mova,
        0x30 => Sm3Opcode::DefI,
        0x41 => Sm3Opcode::TexKill,
        0x42 => Sm3Opcode::TexLd,
        0x51 => Sm3Opcode::Def,
        0x58 => Sm3Opcode::Cmp,
        0x5a => Sm3Opcode::Dp2Add,
        0x5b => Sm3Opcode::Dsx,
        0x5c => Sm3Opcode::Dsy,
        0x5d => Sm3Opcode::TexLdD,
        0x5f => Sm3Opcode::TexLdL,
        value => Sm3Opcode::Unknown(value),
    }
}

fn decode_operation(
    at_word: usize,
    opcode: Sm3Opcode,
    operands: &[u32],
) -> Result<Sm3InstructionBody, Sm3IrError> {
    let expected_sources = match opcode {
        Sm3Opcode::Nop
        | Sm3Opcode::TexKill
        | Sm3Opcode::EndRep
        | Sm3Opcode::Else
        | Sm3Opcode::Endif => 0,
        Sm3Opcode::Mov
        | Sm3Opcode::Rcp
        | Sm3Opcode::Rsq
        | Sm3Opcode::Exp
        | Sm3Opcode::Log
        | Sm3Opcode::Frc
        | Sm3Opcode::Abs
        | Sm3Opcode::Nrm
        | Sm3Opcode::Sincos
        | Sm3Opcode::Mova
        | Sm3Opcode::Dsx
        | Sm3Opcode::Dsy
        | Sm3Opcode::Rep => 1,
        Sm3Opcode::Add
        | Sm3Opcode::Sub
        | Sm3Opcode::Mul
        | Sm3Opcode::Dp3
        | Sm3Opcode::Dp4
        | Sm3Opcode::Min
        | Sm3Opcode::Max
        | Sm3Opcode::Slt
        | Sm3Opcode::Sge
        | Sm3Opcode::Pow
        | Sm3Opcode::TexLd
        | Sm3Opcode::TexLdL
        | Sm3Opcode::Ifc => 2,
        Sm3Opcode::Mad | Sm3Opcode::Lrp | Sm3Opcode::Cmp | Sm3Opcode::Dp2Add => 3,

        Sm3Opcode::TexLdD => 4,
        Sm3Opcode::Dcl | Sm3Opcode::Def | Sm3Opcode::DefI | Sm3Opcode::Unknown(_) => {
            unreachable!("special opcodes are decoded separately")
        }
    };
    let has_destination = !matches!(
        opcode,
        Sm3Opcode::Nop
            | Sm3Opcode::Rep
            | Sm3Opcode::EndRep
            | Sm3Opcode::Ifc
            | Sm3Opcode::Else
            | Sm3Opcode::Endif
    );
    let mut cursor = 0usize;
    let destination = if has_destination {
        Some(decode_destination(at_word, operands, &mut cursor)?)
    } else {
        None
    };
    let mut sources = Vec::new();
    while cursor < operands.len() {
        sources.push(decode_source(at_word, operands, &mut cursor)?);
    }
    if sources.len() != expected_sources {
        return Err(Sm3IrError::InvalidOperandCount {
            at_word,
            opcode,
            expected_sources,
            actual_sources: sources.len(),
        });
    }
    Ok(Sm3InstructionBody::Operation {
        destination,
        sources,
    })
}

fn decode_declaration(at_word: usize, operands: &[u32]) -> Result<Sm3InstructionBody, Sm3IrError> {
    let (usage_token, start) = match operands.len() {
        1 => (None, 0),
        2 => (Some(operands[0]), 1),
        count => {
            return Err(Sm3IrError::InvalidDeclaration {
                at_word,
                operand_count: count,
            });
        }
    };
    let mut cursor = start;
    let destination = decode_destination(at_word, operands, &mut cursor)?;
    if cursor != operands.len() {
        return Err(Sm3IrError::InvalidDeclaration {
            at_word,
            operand_count: operands.len(),
        });
    }
    Ok(Sm3InstructionBody::Declaration {
        usage_token,
        destination,
    })
}

fn decode_definition(at_word: usize, operands: &[u32]) -> Result<Sm3InstructionBody, Sm3IrError> {
    if operands.len() != 5 {
        return Err(Sm3IrError::InvalidDefinition {
            at_word,
            operand_count: operands.len(),
        });
    }
    let mut cursor = 0;
    let destination = decode_destination(at_word, operands, &mut cursor)?;
    Ok(Sm3InstructionBody::FloatDefinition {
        destination,
        values: operands[1..5]
            .try_into()
            .expect("definition length was checked"),
    })
}

fn decode_integer_definition(
    at_word: usize,
    operands: &[u32],
) -> Result<Sm3InstructionBody, Sm3IrError> {
    if operands.len() != 5 {
        return Err(Sm3IrError::InvalidDefinition {
            at_word,
            operand_count: operands.len(),
        });
    }
    let mut cursor = 0;
    let destination = decode_destination(at_word, operands, &mut cursor)?;
    Ok(Sm3InstructionBody::IntegerDefinition {
        destination,
        values: operands[1..5]
            .try_into()
            .expect("definition length was checked"),
    })
}

fn decode_destination(
    at_word: usize,
    operands: &[u32],
    cursor: &mut usize,
) -> Result<Sm3Destination, Sm3IrError> {
    let operand_index = *cursor;
    let token = *operands
        .get(operand_index)
        .ok_or(Sm3IrError::MissingOperand {
            at_word,
            operand: operand_index,
        })?;
    *cursor += 1;
    let relative = decode_relative(at_word, token, operands, cursor)?;
    let modifiers = ((token >> 20) & 0x0f) as u8;
    Ok(Sm3Destination {
        register: decode_register(token),
        write_mask: ((token >> 16) & 0x0f) as u8,
        saturate: modifiers & 1 != 0,
        partial_precision: modifiers & 2 != 0,
        centroid: modifiers & 4 != 0,
        unknown_modifier_bits: modifiers & !7,
        relative,
    })
}

fn decode_source(
    at_word: usize,
    operands: &[u32],
    cursor: &mut usize,
) -> Result<Sm3Source, Sm3IrError> {
    let operand_index = *cursor;
    let token = *operands
        .get(operand_index)
        .ok_or(Sm3IrError::MissingOperand {
            at_word,
            operand: operand_index,
        })?;
    *cursor += 1;
    let relative = decode_relative(at_word, token, operands, cursor)?;
    let swizzle = (token >> 16) as u8;
    Ok(Sm3Source {
        register: decode_register(token),
        swizzle: [
            swizzle & 3,
            (swizzle >> 2) & 3,
            (swizzle >> 4) & 3,
            swizzle >> 6,
        ],
        modifier: decode_source_modifier(((token >> 24) & 0x0f) as u8),
        relative,
    })
}

fn decode_relative(
    at_word: usize,
    token: u32,
    operands: &[u32],
    cursor: &mut usize,
) -> Result<Option<Sm3RelativeAddress>, Sm3IrError> {
    if token & 0x0000_2000 == 0 {
        return Ok(None);
    }
    let operand_index = *cursor;
    let token = *operands
        .get(operand_index)
        .ok_or(Sm3IrError::MalformedRelativeAddress {
            at_word,
            operand: operand_index,
        })?;
    *cursor += 1;
    Ok(Some(Sm3RelativeAddress {
        register: decode_register(token),
        component: ((token >> 16) & 3) as u8,
    }))
}

fn decode_register(token: u32) -> Sm3Register {
    let raw_file = (((token >> 28) & 7) | ((token >> 8) & 0x18)) as u8;
    let file = match raw_file {
        0 => Sm3RegisterFile::Temporary,
        1 => Sm3RegisterFile::Input,
        2 | 11..=13 => Sm3RegisterFile::FloatConstant,
        3 => Sm3RegisterFile::Texture,
        4 => Sm3RegisterFile::RasterOutput,
        5 => Sm3RegisterFile::AttributeOutput,
        6 => Sm3RegisterFile::Output,
        7 => Sm3RegisterFile::IntegerConstant,
        8 => Sm3RegisterFile::ColourOutput,
        9 => Sm3RegisterFile::DepthOutput,
        10 => Sm3RegisterFile::Sampler,
        14 => Sm3RegisterFile::BooleanConstant,
        15 => Sm3RegisterFile::Loop,
        16 => Sm3RegisterFile::Float16Temporary,
        17 => Sm3RegisterFile::Misc,
        18 => Sm3RegisterFile::Label,
        19 => Sm3RegisterFile::Predicate,
        value => Sm3RegisterFile::Unknown(value),
    };
    Sm3Register {
        file,
        index: (token & 0x7ff) as u16,
    }
}

fn decode_source_modifier(raw: u8) -> Sm3SourceModifier {
    match raw {
        0 => Sm3SourceModifier::None,
        1 => Sm3SourceModifier::Negate,
        2 => Sm3SourceModifier::Bias,
        3 => Sm3SourceModifier::BiasNegate,
        4 => Sm3SourceModifier::Sign,
        5 => Sm3SourceModifier::SignNegate,
        6 => Sm3SourceModifier::Complement,
        7 => Sm3SourceModifier::Double,
        8 => Sm3SourceModifier::DoubleNegate,
        9 => Sm3SourceModifier::DivideByZ,
        10 => Sm3SourceModifier::DivideByW,
        11 => Sm3SourceModifier::Absolute,
        12 => Sm3SourceModifier::AbsoluteNegate,
        13 => Sm3SourceModifier::LogicalNot,
        value => Sm3SourceModifier::Unknown(value),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sm3LoweringRefusal {
    UnknownOpcode { at_word: usize, opcode: u16 },
    Predication { at_word: usize },
    Coissue { at_word: usize },
    RelativeAddressing { at_word: usize },
    UnknownRegisterFile { at_word: usize, file: u8 },
    UnknownSourceModifier { at_word: usize, modifier: u8 },
    UnknownDestinationModifier { at_word: usize, bits: u8 },
}

pub fn validate_supported_opcode_surface(program: &Sm3ProgramIr) -> Result<(), Sm3LoweringRefusal> {
    for instruction in &program.instructions {
        if instruction.predicated {
            return Err(Sm3LoweringRefusal::Predication {
                at_word: instruction.at_word,
            });
        }
        if instruction.coissue {
            return Err(Sm3LoweringRefusal::Coissue {
                at_word: instruction.at_word,
            });
        }
        if let Sm3Opcode::Unknown(opcode) = instruction.opcode {
            return Err(Sm3LoweringRefusal::UnknownOpcode {
                at_word: instruction.at_word,
                opcode,
            });
        }
        refuse_unsupported_relative(program.stage, instruction)?;
        visit_operands(instruction, |register, _relative, modifier| {
            if let Sm3RegisterFile::Unknown(file) = register.file {
                return Err(Sm3LoweringRefusal::UnknownRegisterFile {
                    at_word: instruction.at_word,
                    file,
                });
            }
            if let Some(Sm3SourceModifier::Unknown(modifier)) = modifier {
                return Err(Sm3LoweringRefusal::UnknownSourceModifier {
                    at_word: instruction.at_word,
                    modifier,
                });
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn refuse_unsupported_relative(
    stage: ShaderStage,
    instruction: &Sm3Instruction,
) -> Result<(), Sm3LoweringRefusal> {
    let at_word = instruction.at_word;
    let refuse = || Err(Sm3LoweringRefusal::RelativeAddressing { at_word });
    match &instruction.body {
        Sm3InstructionBody::Operation {
            destination,
            sources,
        } => {
            if destination
                .as_ref()
                .is_some_and(|destination| destination.relative.is_some())
            {
                return refuse();
            }
            for source in sources {
                if source.relative.is_none() {
                    continue;
                }
                let supported = stage == ShaderStage::Vertex
                    && source.register.file == Sm3RegisterFile::FloatConstant
                    && source
                        .relative
                        .is_some_and(|relative| relative.register.file == Sm3RegisterFile::Address);
                if !supported {
                    return refuse();
                }
            }
            Ok(())
        }
        Sm3InstructionBody::Declaration { destination, .. }
        | Sm3InstructionBody::FloatDefinition { destination, .. }
        | Sm3InstructionBody::IntegerDefinition { destination, .. } => {
            if destination.relative.is_some() {
                refuse()
            } else {
                Ok(())
            }
        }
        Sm3InstructionBody::Unmodeled { .. } => Ok(()),
    }
}

fn visit_operands(
    instruction: &Sm3Instruction,
    mut visitor: impl FnMut(
        Sm3Register,
        Option<Sm3RelativeAddress>,
        Option<Sm3SourceModifier>,
    ) -> Result<(), Sm3LoweringRefusal>,
) -> Result<(), Sm3LoweringRefusal> {
    let mut visit_destination = |destination: Sm3Destination| {
        if destination.unknown_modifier_bits != 0 {
            return Err(Sm3LoweringRefusal::UnknownDestinationModifier {
                at_word: instruction.at_word,
                bits: destination.unknown_modifier_bits,
            });
        }
        visitor(destination.register, destination.relative, None)
    };
    match &instruction.body {
        Sm3InstructionBody::Operation {
            destination,
            sources,
        } => {
            if let Some(destination) = destination {
                visit_destination(*destination)?;
            }
            for source in sources {
                visitor(source.register, source.relative, Some(source.modifier))?;
            }
        }
        Sm3InstructionBody::Declaration { destination, .. }
        | Sm3InstructionBody::FloatDefinition { destination, .. }
        | Sm3InstructionBody::IntegerDefinition { destination, .. } => {
            visit_destination(*destination)?;
        }
        Sm3InstructionBody::Unmodeled { .. } => {}
    }
    Ok(())
}
