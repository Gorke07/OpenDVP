//! ES2 simulator instruction words recovered from the original decoder.
//! This is an in-memory simulator program encoder, not a `.dvp` file writer.
use crate::plc::{
    Action, Area, Device, Expr, Flow, Operand, Program, WordConversion, WordLogic, WordUnary,
};
use crate::shift::{Rotation, ShiftAction};
use crate::wide::{WideOp, WideOperand};

pub const MAX_PROGRAM_WORDS: usize = 15_872;
/// Source/expanded instruction budget: bounded only by the program region (one WORD at least
/// per instruction), not by an arbitrary first-version cap.
pub const MAX_EXECUTABLE_INSTRUCTIONS: usize = MAX_PROGRAM_WORDS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightCheck {
    Matrix(usize),
    Drum(usize),
    CalendarClock(usize),
    Solar(usize),
    Search(usize),
    Transform(usize),
    Sort(usize),
    Text(usize),
    IndexedScalar(usize),
    IndexedMultiply(usize),
    IndexedFloat(usize),
    IndexedRoot(usize),
    IndexedAngle(usize),
    IndexedDecimalConversion(usize),
    IndexedExpLn(usize),
    IndexedLogPow(usize),
    IndexedDesqr(usize),
    IndexedTrig(usize),
    IndexedFloatComparison(usize),
    IndexedLiteralMove(usize),
    IndexedSwapMask(usize),
    IndexedRandom(usize),
    IndexedBitNumberAction(usize),
    IndexedBitNumberPredicate(usize),
    IndexedAbsGray(usize),
    IndexedSearchToggle(usize),
    IndexedCalendarTime(usize),
    IndexedRtc(usize),
    IndexedSegment(usize),
    IndexedChecksum(usize),
    IndexedText(usize),
    IndexedAdditional(usize),
    IndexedDataProcessing(usize),
    IndexedShiftFifo(usize),
    IndexedBlockExchange(usize),
    IndexedRotation(usize),
    IndexedRealBinary(usize),
    IndexedFloatBinary(usize),
    IndexedBcd(usize),
    IndexedTruncated(usize),
    IndexedComparison(usize),
    IndexedPredicate(usize),
    IndexedFloatPredicate(usize),
    IndexedBitAction(usize),
    IndexedBitContact(usize),
    WordCounter(usize),
    NumericReset(usize),
    DoubleCounter(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EncodedProgram {
    pub execution_plan: crate::execution_plan::ExecutionPlan,
    pub preflight: std::collections::BTreeMap<usize, PreflightCheck>,
    /// DDIVR's installed NaN path overshoots four words. Four balanced INV
    /// instructions preserve normal power and provide an exact safe landing.
    pub padded_error_skip: std::collections::BTreeMap<usize, u16>,
    /// Exact six-instruction skip declarations: ORZ uses balanced INV padding;
    /// DRAND's disabled handler advances seven words; its six real operand WORDs
    /// then dispatch individually. Enabled/latched-P DRAND skips all six.
    pub padded_short_circuit: std::collections::BTreeMap<usize, u16>,
    pub action_pc: Vec<u16>,
    pub words: Vec<u16>,
    /// Expected PC after each emitted executable instruction. The final END is 0.
    /// One compound source expression can emit several executable instructions.
    pub next_pc: Vec<u16>,
}

/// An instruction interval whose length is known without a usable wire
/// encoding. Keys in LocalExecutionLayout::unencoded_ranges are start WORD PCs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnencodedRange {
    pub word_count: u16,
    pub reason: String,
}

/// Source-VM positions, deliberately not uploadable instruction bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalExecutionLayout {
    pub word_count: u16,
    pub action_pc: Vec<u16>,
    pub execution_plan: crate::execution_plan::ExecutionPlan,
    pub unencoded_ranges: std::collections::BTreeMap<u16, UnencodedRange>,
    known_words: Vec<Option<u16>>,
}
impl LocalExecutionLayout {
    /// None means either an unencoded local interval or an out-of-layout PC.
    /// Some is an emitted word, not a claim that native execution is supported.
    pub fn known_word_at(&self, pc: u16) -> Option<u16> {
        self.known_words.get(usize::from(pc)).copied().flatten()
    }
}
struct BuiltLayout {
    encoded: EncodedProgram,
    unencoded_ranges: std::collections::BTreeMap<u16, UnencodedRange>,
}

pub fn encode(program: &Program) -> Result<EncodedProgram, String> {
    encode_for_transform_profile(
        program,
        crate::data_transform::NativeTransformProfile::InstalledLegacy,
    )
}

/// Preserve the legacy default; a verified simulator session supplies the
/// profile whose DTM selector bodies have been independently inspected.
pub fn encode_for_transform_profile(
    program: &Program,
    transform_profile: crate::data_transform::NativeTransformProfile,
) -> Result<EncodedProgram, String> {
    let built = encode_layout(program, false, transform_profile)?;
    if !built.unencoded_ranges.is_empty() {
        return Err("Native encoding cannot contain unencoded local intervals".into());
    }
    Ok(built.encoded)
}

/// Build typed source-VM positions without exporting placeholder bytecode.
pub(crate) fn local_execution_layout(program: &Program) -> Result<LocalExecutionLayout, String> {
    let built = encode_layout(
        program,
        true,
        crate::data_transform::NativeTransformProfile::InstalledLegacy,
    )?;
    let mut known_words: Vec<_> = built.encoded.words.iter().copied().map(Some).collect();
    for (&pc, range) in &built.unencoded_ranges {
        let start = usize::from(pc);
        known_words[start..start + usize::from(range.word_count)].fill(None);
    }
    Ok(LocalExecutionLayout {
        word_count: u16::try_from(known_words.len())
            .map_err(|_| "Local layout WORD count overflow")?,
        action_pc: built.encoded.action_pc,
        execution_plan: built.encoded.execution_plan,
        unencoded_ranges: built.unencoded_ranges,
        known_words,
    })
}

fn encode_layout(
    program: &Program,
    local_layout: bool,
    transform_profile: crate::data_transform::NativeTransformProfile,
) -> Result<BuiltLayout, String> {
    if program.instructions > MAX_EXECUTABLE_INSTRUCTIONS {
        return Err(format!("ES2 encoder supports at most {MAX_EXECUTABLE_INSTRUCTIONS} source instructions"));
    }
    let mut emitter = Emitter {
        local_layout,
        transform_profile,
        unencoded_ranges: Default::default(),
        pending: None,
        continuations: Vec::new(),
        output: EncodedProgram {
            execution_plan: Default::default(),
            preflight: Default::default(),
            padded_error_skip: Default::default(),
            padded_short_circuit: Default::default(),
            action_pc: Vec::new(),
            words: Vec::new(),
            next_pc: Vec::new(),
        },
    };
    for step in &program.flow {
        let network = match step {
            Flow::Evaluate(condition) => {
                emitter.expression(condition, 0)?;
                continue;
            }
            Flow::Capture(id) => {
                emitter.pending = Some(crate::execution_plan::PlanOp::Capture(*id));
                emitter.instruction(&[0xfaf0])?;
                continue;
            }
            Flow::Restore(pop) => {
                emitter.pending = Some(crate::execution_plan::PlanOp::Restore(*pop));
                emitter.instruction(&[if *pop { 0xfcf0 } else { 0xfbf0 }])?;
                continue;
            }
            Flow::Control { op, line } => {
                if !local_layout
                    && matches!(
                        op,
                        crate::program_control::ControlOp::InterruptLabel(_)
                            | crate::program_control::ControlOp::InterruptReturn
                            | crate::program_control::ControlOp::InterruptEnable
                            | crate::program_control::ControlOp::InterruptDisable
                            | crate::program_control::ControlOp::Watchdog { .. }
                    )
                {
                    return Err("Kesme/watchdog yerel olay ve zaman profili kullanır; kurulu ES2/SS2 tek-komut karşılaştırması bu asenkron yürütmeyi henüz doğrulamaz".into());
                }
                emitter.pending = Some(crate::execution_plan::PlanOp::Control {
                    op: op.clone(),
                    line: *line,
                });
                let mut words = vec![op.header()];
                if let crate::program_control::ControlOp::For { count } = op {
                    words.extend(matrix_scalar_operand(count)?);
                } else if let Some(operand) = op.native_word_operand()? {
                    words.extend(operand);
                }
                emitter.instruction(&words)?;
                continue;
            }
            Flow::Network(index) => &program.networks[*index],
        };
        if network.actions.is_empty() {
            return Err(format!(
                "Network at source line {} has no action",
                network.line
            ));
        }
        emitter.expression(&network.condition, 0)?;
        for (action_index, action) in network.actions.iter().enumerate() {
            let network_index = match step {
                Flow::Network(index) => *index,
                _ => unreachable!(),
            };
            emitter.pending = Some(crate::execution_plan::PlanOp::Action {
                network: network_index,
                action: action_index,
                ordinal: emitter.output.action_pc.len(),
            });
            emitter
                .output
                .action_pc
                .push(emitter.output.words.len() as u16);
            emitter.action(action)?;
        }
    }
    emitter.pending = Some(crate::execution_plan::PlanOp::End);
    emitter.instruction(&[0x0F00])?;
    emitter.output.execution_plan.link()?;
    crate::random_tail::validate_layout(program, &emitter.output)?;
    crate::sfc_validation::validate(program, &emitter.output.execution_plan)?;
    *emitter.output.next_pc.last_mut().unwrap() = 0;
    Ok(BuiltLayout {
        encoded: emitter.output,
        unencoded_ranges: emitter.unencoded_ranges,
    })
}

struct Emitter {
    output: EncodedProgram,
    unencoded_ranges: std::collections::BTreeMap<u16, UnencodedRange>,
    local_layout: bool,
    transform_profile: crate::data_transform::NativeTransformProfile,
    pending: Option<crate::execution_plan::PlanOp>,
    continuations: Vec<usize>,
}
impl Emitter {
    /// Reserve ONE typed source action. Internal zeros are cursor storage only:
    /// LocalExecutionLayout hides them, and native mode cannot call this path.
    fn reserve_local_instruction(
        &mut self,
        word_count: u16,
        reason: impl Into<String>,
    ) -> Result<(), String> {
        if !self.local_layout {
            return Err("A local-only instruction has no native encoding".into());
        }
        if word_count == 0 {
            return Err("A local instruction must occupy at least one WORD".into());
        }
        if !matches!(
            self.pending,
            Some(crate::execution_plan::PlanOp::Action { .. })
        ) {
            return Err("A local interval requires an explicit typed action".into());
        }
        let start = self.output.words.len();
        self.instruction(&vec![0; usize::from(word_count)])?;
        self.unencoded_ranges.insert(
            start as u16,
            UnencodedRange {
                word_count,
                reason: reason.into(),
            },
        );
        Ok(())
    }
    fn instruction(&mut self, words: &[u16]) -> Result<(), String> {
        if words.is_empty() {
            return Err("An executable instruction cannot have zero WORD length".into());
        }
        if !self.local_layout && self.output.next_pc.len() >= MAX_EXECUTABLE_INSTRUCTIONS {
            return Err(format!("Expanded ES2 program exceeds {MAX_EXECUTABLE_INSTRUCTIONS} executable instructions"));
        }
        let end = self
            .output
            .words
            .len()
            .checked_add(words.len())
            .filter(|end| *end <= MAX_PROGRAM_WORDS && *end <= usize::from(u16::MAX))
            .ok_or("ES2 program exceeds the 15872-word program region")?;
        let pc = self.output.words.len() as u16;
        let fallthrough = end as u16;
        self.output.execution_plan.push(
            pc,
            fallthrough,
            self.pending
                .take()
                .unwrap_or(crate::execution_plan::PlanOp::Inert),
        )?;
        self.output
            .execution_plan
            .nodes
            .last_mut()
            .expect("just inserted instruction")
            .continuations = std::mem::take(&mut self.continuations);
        self.output.words.extend_from_slice(words);
        self.output.next_pc.push(self.output.words.len() as u16);
        Ok(())
    }

    fn drand_instruction(&mut self, words: &[u16]) -> Result<(), String> {
        let tail = crate::random_tail::decode_drand_tail(words)?;
        let index = self.output.next_pc.len();
        let header = self.output.words.len() as u16;
        self.output
            .execution_plan
            .operand_tail_owners
            .insert(header, crate::random_tail::OperandTailOwner::Drand);
        self.instruction(&words[..7])?;
        for (word, op) in words[7..].iter().zip(tail) {
            self.pending = Some(match op {
                Some(op) => crate::execution_plan::PlanOp::OperandTailControl { header, op },
                None => crate::execution_plan::PlanOp::Inert,
            });
            self.instruction(&[*word])?;
        }
        self.output.padded_short_circuit.insert(index, header + 13);
        Ok(())
    }
    fn time_arithmetic_instruction(&mut self, words: &[u16]) -> Result<(), String> {
        let tail = crate::calendar_tail::decode_time_tail(words)?;
        let header = self.output.words.len() as u16;
        self.output
            .execution_plan
            .operand_tail_owners
            .insert(header, crate::random_tail::OperandTailOwner::CalendarTime);
        self.instruction(&words[..3])?;
        for (word, op) in words[3..].iter().zip(tail) {
            self.pending = Some(match op {
                Some(op) => crate::execution_plan::PlanOp::OperandTailControl { header, op },
                None => crate::execution_plan::PlanOp::Inert,
            });
            self.instruction(&[*word])?;
        }
        Ok(())
    }
    fn expression(&mut self, expression: &Expr, depth: usize) -> Result<(), String> {
        // Programs normally originate in the bounded parser. Keep this public
        // encoder bounded even for a manually constructed expression tree.
        if depth >= MAX_EXECUTABLE_INSTRUCTIONS {
            return Err("ES2 expression nesting exceeds the encoder limit".into());
        }
        use crate::execution_plan::{LogicMode, PlanOp};
        if matches!(
            expression,
            Expr::Float(_)
                | Expr::IndexedFloatComparison(_)
                | Expr::IndexedBitNumber(_)
                | Expr::IndexedComparison(_)
                | Expr::IndexedBit(_)
                | Expr::Numeric(_)
                | Expr::CompareWide(..)
                | Expr::Compare(..)
                | Expr::Contact(..)
        ) {
            self.pending = Some(PlanOp::Predicate {
                mode: LogicMode::Load,
                expr: expression.clone(),
            });
        }
        match expression {
            Expr::IndexedBitNumber(predicate) => self.indexed_bit_number_predicate("LD", predicate),
            Expr::IndexedBit(contact) => self.indexed_bit_contact("LD", contact),
            Expr::IndexedComparison(predicate) => self.indexed_predicate("LD", predicate),
            Expr::IndexedFloatComparison(predicate) => {
                self.indexed_float_predicate("LD", predicate)
            }
            Expr::Float(predicate) => self.float_contact("LD", predicate),
            Expr::Numeric(predicate) => self.numeric_contact("LD", predicate),
            Expr::Invert(inner) => {
                self.expression(inner, depth + 1)?;
                self.pending = Some(PlanOp::Invert);
                self.instruction(&[0xfdf0])
            }
            Expr::CompareWide(op, a, b) => {
                self.comparison_wide(0xd101 + op.opcode_offset() * 0x100, a, b)
            }
            Expr::Compare(op, a, b) => self.comparison(0xd001 + op.opcode_offset() * 0x100, a, b),
            Expr::Saved(id) => {
                if *id > usize::MAX / 2 {
                    self.continuations.push(*id);
                }
                Ok(())
            }
            Expr::Contact(device, inverted) => {
                self.bit_instruction(if *inverted { BitOp::Ldi } else { BitOp::Ld }, *device)
            }
            Expr::Edge(d, rising, _) => {
                // Semantic lowering: sample this operand independently, then
                // apply the original's per-PC result-edge instruction.
                self.pending = Some(PlanOp::Predicate {
                    mode: LogicMode::Load,
                    expr: Expr::Contact(*d, false),
                });
                self.bit_instruction(BitOp::Ld, *d)?;
                self.pending = Some(PlanOp::Edge {
                    rising: *rising,
                    id: if let Expr::Edge(_, _, id) = expression {
                        *id
                    } else {
                        unreachable!()
                    },
                });
                self.instruction(&[if *rising { 0xf6f0 } else { 0xf7f0 }])
            }
            Expr::Trigger(inner, rising, _) => {
                self.expression(inner, depth + 1)?;
                self.pending = Some(PlanOp::Edge {
                    rising: *rising,
                    id: if let Expr::Trigger(_, _, id) = expression {
                        *id
                    } else {
                        unreachable!()
                    },
                });
                // NP/PN apply to the complete preceding logic result.
                self.instruction(&[if *rising { 0xf6f0 } else { 0xf7f0 }])
            }
            Expr::And(a, b) | Expr::Or(a, b) => {
                self.expression(a, depth + 1)?;
                if matches!(
                    b.as_ref(),
                    Expr::Float(_)
                        | Expr::IndexedFloatComparison(_)
                        | Expr::Numeric(_)
                        | Expr::CompareWide(..)
                        | Expr::Compare(..)
                        | Expr::IndexedBitNumber(_)
                        | Expr::IndexedComparison(_)
                        | Expr::IndexedBit(_)
                ) {
                    self.pending = Some(PlanOp::Predicate {
                        mode: if matches!(expression, Expr::And(..)) {
                            LogicMode::And
                        } else {
                            LogicMode::Or
                        },
                        expr: b.as_ref().clone(),
                    });
                }
                if let Expr::IndexedBitNumber(predicate) = b.as_ref() {
                    return self.indexed_bit_number_predicate(
                        if matches!(expression, Expr::And(..)) {
                            "AND"
                        } else {
                            "OR"
                        },
                        predicate,
                    );
                }
                if let Expr::IndexedBit(contact) = b.as_ref() {
                    return self.indexed_bit_contact(
                        if matches!(expression, Expr::And(..)) {
                            "AND"
                        } else {
                            "OR"
                        },
                        contact,
                    );
                }
                if let Expr::IndexedComparison(predicate) = b.as_ref() {
                    return self.indexed_predicate(
                        if matches!(expression, Expr::And(..)) {
                            "AND"
                        } else {
                            "OR"
                        },
                        predicate,
                    );
                }
                if let Expr::IndexedFloatComparison(predicate) = b.as_ref() {
                    return self.indexed_float_predicate(
                        if matches!(expression, Expr::And(..)) {
                            "AND"
                        } else {
                            "OR"
                        },
                        predicate,
                    );
                }
                if let Expr::Float(predicate) = b.as_ref() {
                    return self.float_contact(
                        if matches!(expression, Expr::And(..)) {
                            "AND"
                        } else {
                            "OR"
                        },
                        predicate,
                    );
                }
                if let Expr::Numeric(predicate) = b.as_ref() {
                    return self.numeric_contact(
                        if matches!(expression, Expr::And(..)) {
                            "AND"
                        } else {
                            "OR"
                        },
                        predicate,
                    );
                }
                if let Expr::CompareWide(op, left, right) = b.as_ref() {
                    return self.comparison_wide(
                        (if matches!(expression, Expr::And(..)) {
                            0xe101
                        } else {
                            0xf101
                        }) + op.opcode_offset() * 0x100,
                        left,
                        right,
                    );
                }
                if let Expr::Compare(op, left, right) = b.as_ref() {
                    return self.comparison(
                        (if matches!(expression, Expr::And(..)) {
                            0xe001
                        } else {
                            0xf001
                        }) + op.opcode_offset() * 0x100,
                        left,
                        right,
                    );
                }
                self.expression(b, depth + 1)?;
                self.pending = Some(if matches!(expression, Expr::And(..)) {
                    PlanOp::AndBlock
                } else {
                    PlanOp::OrBlock
                });
                self.instruction(&[if matches!(expression, Expr::And(..)) {
                    0xF8F0
                } else {
                    0xF9F0
                }])
            }
        }
    }

    fn indexed_bit_number_predicate(
        &mut self,
        prefix: &str,
        predicate: &crate::indexed_bit_number::IndexedBitNumberPredicate,
    ) -> Result<(), String> {
        let index = self.output.next_pc.len();
        self.output
            .preflight
            .insert(index, PreflightCheck::IndexedBitNumberPredicate(index));
        self.instruction(&predicate.native_words(prefix)?)
    }

    fn indexed_bit_contact(
        &mut self,
        prefix: &str,
        contact: &crate::indexed_bit::IndexedBitContact,
    ) -> Result<(), String> {
        let index = self.output.next_pc.len();
        self.output
            .preflight
            .insert(index, PreflightCheck::IndexedBitContact(index));
        self.instruction(&contact.native_words(prefix)?)
    }

    fn indexed_predicate(
        &mut self,
        prefix: &str,
        predicate: &crate::indexed_comparison::IndexedPredicate,
    ) -> Result<(), String> {
        let index = self.output.next_pc.len();
        self.output
            .preflight
            .insert(index, PreflightCheck::IndexedPredicate(index));
        self.instruction(&predicate.native_words(prefix)?)?;
        if predicate.needs_or_padding(prefix) {
            let target = self.output.words.len() as u16 + 6;
            self.output.padded_short_circuit.insert(index, target);
            self.output.execution_plan.nodes[index].shortcut_pc = Some(target);
            for _ in 0..6 {
                self.instruction(&[0xfdf0])?;
            }
        }
        Ok(())
    }

    fn comparison(
        &mut self,
        header: u16,
        a: &crate::calendar_operand::CalendarValue,
        b: &crate::calendar_operand::CalendarValue,
    ) -> Result<(), String> {
        let a = calendar_operand(a)?;
        let b = calendar_operand(b)?;
        self.instruction(&[header, a[0], a[1], b[0], b[1]])
    }

    fn indexed_float_predicate(
        &mut self,
        prefix: &str,
        predicate: &crate::indexed_float_comparison::IndexedFloatPredicate,
    ) -> Result<(), String> {
        let index = self.output.next_pc.len();
        self.output
            .preflight
            .insert(index, PreflightCheck::IndexedFloatPredicate(index));
        self.instruction(&predicate.native_words(prefix)?)
    }

    fn float_contact(
        &mut self,
        prefix: &str,
        predicate: &crate::float_ops::FloatPredicate,
    ) -> Result<(), String> {
        let base = match prefix {
            "LD" => 0x47,
            "AND" => 0x53,
            "OR" => 0x5f,
            _ => return Err("Invalid float contact prefix".into()),
        };
        let offset = match predicate.relation {
            crate::plc::Relation::Eq => 0,
            crate::plc::Relation::Gt => 2,
            crate::plc::Relation::Lt => 4,
            crate::plc::Relation::Ne => 6,
            crate::plc::Relation::Le => 8,
            crate::plc::Relation::Ge => 10,
        };
        let mut words = vec![((base + offset) << 8) | 2];
        words.extend(float_operand(&predicate.left)?);
        words.extend(float_operand(&predicate.right)?);
        self.instruction(&words)
    }

    fn numeric_contact(
        &mut self,
        prefix: &str,
        predicate: &crate::numeric_contacts::NumericPredicate,
    ) -> Result<(), String> {
        use crate::numeric_contacts::NumericPredicate;
        use crate::plc::{Relation, WordLogic};
        let wide = u16::from(predicate.width() == 32);
        let mut words = match predicate {
            NumericPredicate::Indexed {
                source, inverted, ..
            } => {
                let base = match (prefix, inverted) {
                    ("LD", false) => 0x3a,
                    ("LD", true) => 0x3c,
                    ("AND", false) => 0x3e,
                    ("AND", true) => 0x40,
                    ("OR", false) => 0x42,
                    ("OR", true) => 0x44,
                    _ => return Err("Invalid indexed contact prefix".into()),
                };
                vec![((base + u16::from(source.wide())) << 8) | 2]
            }
            NumericPredicate::Logic(op, _) | NumericPredicate::LogicWide(op, _) => {
                let base = match prefix {
                    "LD" => 0xbe,
                    "AND" => 0xc4,
                    "OR" => 0xca,
                    _ => return Err("Invalid numeric contact prefix".into()),
                };
                let offset = match op {
                    WordLogic::And => 0,
                    WordLogic::Or => 2,
                    WordLogic::Xor => 4,
                };
                vec![((base + offset + wide) << 8) | 1]
            }
            NumericPredicate::Difference(op, _) | NumericPredicate::DifferenceWide(op, _) => {
                let base = match prefix {
                    "LD" => 0x70,
                    "AND" => 0x7c,
                    "OR" => 0x88,
                    _ => return Err("Invalid numeric contact prefix".into()),
                };
                let offset = match op {
                    Relation::Gt => 0,
                    Relation::Ge => 2,
                    Relation::Lt => 4,
                    Relation::Le => 6,
                    Relation::Eq => 8,
                    Relation::Ne => 10,
                };
                vec![((base + offset + wide) << 8) | 2]
            }
        };
        match predicate {
            NumericPredicate::Indexed { source, index, .. } => {
                words.extend(calendar_operand(source)?);
                words.extend(bit_index_operand(index)?);
            }
            NumericPredicate::Logic(_, sources) | NumericPredicate::LogicWide(_, sources) => {
                for source in sources {
                    words.extend(calendar_operand(source)?);
                }
            }
            NumericPredicate::Difference(_, sources)
            | NumericPredicate::DifferenceWide(_, sources) => {
                for source in sources {
                    words.extend(additional_operand(source)?);
                }
            }
        }
        let index = self.output.next_pc.len();
        self.instruction(&words)?;
        if prefix == "OR" && matches!(predicate, NumericPredicate::Difference(..)) {
            // Retain the dedicated ORZ opcode. Its true shortcut must land
            // after the padding, while false evaluates seven words normally.
            let target = self.output.words.len() as u16 + 6;
            self.output.padded_short_circuit.insert(index, target);
            self.output.execution_plan.nodes[index].shortcut_pc = Some(target);
            for _ in 0..6 {
                self.instruction(&[0xfdf0])?;
            }
        }
        Ok(())
    }
    fn comparison_wide(
        &mut self,
        header: u16,
        a: &crate::calendar_operand::CalendarValue,
        b: &crate::calendar_operand::CalendarValue,
    ) -> Result<(), String> {
        let mut words = vec![header];
        words.extend(calendar_operand(a)?);
        words.extend(calendar_operand(b)?);
        self.instruction(&words)
    }

    fn bit_instruction(&mut self, operation: BitOp, device: Device) -> Result<(), String> {
        let address = internal_bit_address(device)?;
        let opcode = match operation {
            BitOp::Ld => 0x20,
            BitOp::Ldi => 0x30,
            BitOp::Out => 0xC0,
            BitOp::Set => 0xD0,
        };
        if address <= 0x0FFF {
            self.instruction(&[((address & 0xFF) << 8) | opcode | (address >> 8)])
        } else {
            let header = match operation {
                BitOp::Ld => 0x0202,
                BitOp::Ldi => 0x0302,
                BitOp::Out => 0x0C02,
                BitOp::Set => 0x0D02,
            };
            let pair = bit_operand(address);
            self.instruction(&[header, pair[0], pair[1]])
        }
    }

    fn action(&mut self, action: &Action) -> Result<(), String> {
        match action {
            Action::IndexedBit(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedBitAction(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedAngle(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedAngle(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedDecimalConversion(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedDecimalConversion(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedExpLn(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedExpLn(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedLogPow(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedLogPow(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedDesqr(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedDesqr(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedTrig(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedTrig(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedFloatComparison(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedFloatComparison(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedLiteralMove(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedLiteralMove(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedSwapMask(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedSwapMask(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedRandom(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedRandom(self.output.action_pc.len() - 1),
                );
                if action.wide() {
                    self.drand_instruction(&action.native_words()?)
                } else {
                    self.instruction(&action.native_words()?)
                }
            }
            Action::IndexedBitNumber(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedBitNumberAction(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedAbsGray(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedAbsGray(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedSearchToggle(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedSearchToggle(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedAsc(action) => {
                if self.local_layout {
                    return self.reserve_local_instruction(
                        action.nominal_words(),
                        "ASC indexed: local instruction span only",
                    );
                }
                self.instruction(&action.native_words()?)
            }
            Action::IndexedAdditional(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedAdditional(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedText(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedText(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedChecksum(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedChecksum(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedSegment(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedSegment(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedRtc(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedRtc(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedCalendarTime(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedCalendarTime(self.output.action_pc.len() - 1),
                );
                let words = action.native_words()?;
                if action.is_arithmetic() {
                    self.time_arithmetic_instruction(&words)
                } else {
                    self.instruction(&words)
                }
            }
            Action::IndexedDataProcessing(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedDataProcessing(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedShiftFifo(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedShiftFifo(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedBlockExchange(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedBlockExchange(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedRotation(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedRotation(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedRealBinary(action) => {
                let index = self.output.next_pc.len();
                self.output.preflight.insert(
                    index,
                    PreflightCheck::IndexedRealBinary(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)?;
                if action.is_divide_real() {
                    // The installed NaN branch exits at header+17; all other
                    // paths reach +13 and execute four balanced INV words.
                    let target = self.output.words.len() as u16 + 4;
                    self.output.padded_error_skip.insert(index, target);
                    for _ in 0..4 {
                        self.instruction(&[0xfdf0])?;
                    }
                }
                Ok(())
            }
            Action::IndexedFloatBinary(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedFloatBinary(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedRoot(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedRoot(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedFloat(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedFloat(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedBcd(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedBcd(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedTruncated(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedTruncated(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedMultiply(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedMultiply(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedScalar(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedScalar(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::IndexedComparison(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedComparison(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::Refresh(action) => {
                if let crate::refresh_io::RefreshAction::Filter { milliseconds } = action {
                    let operand = numeric_operand(&Operand::Constant(*milliseconds as i16))?;
                    return self.instruction(&[action.header(), operand[0], operand[1]]);
                }
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 yordamı yalnız PC ilerletiyor; yerel fiziksel giriş/çıkış profili native eşdeğerlik kanıtı değildir",
                        action.name()
                    ));
                }
                self.reserve_local_instruction(
                    if matches!(action, crate::refresh_io::RefreshAction::Matrix { .. }) {
                        9
                    } else {
                        5
                    },
                    "REF/MTR: local model reserves the documented instruction span",
                )
            }
            Action::ParallelRun(action) => {
                if !self.local_layout {
                    action.validate_native_span(&Default::default())?;
                }
                let mut words = vec![action.header()];
                for operand in action.operands() {
                    words.extend(additional_operand(&operand)?);
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::ExtendedMotion(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 yordamı yalnız program sayacını ilerletiyor; hareket algoritması karşılaştırması yok",
                        action.name()
                    ));
                }
                let mut words = vec![action.header()];
                for operand in action.operands() {
                    match operand {
                        crate::motion_basic::MotionOperand::Value(value) => {
                            words.extend(calendar_operand(&value)?)
                        }
                        crate::motion_basic::MotionOperand::Bit(device) => {
                            words.extend(bit_operand(internal_bit_address(device)?));
                            words.extend([0x0080, 0x0080]);
                        }
                    }
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::Dmv(action) => {
                if !self.local_layout {
                    return Err(
                        "DMVRW: native ES2/SS2 gövdesi 9 yerine yalnız 1 word ilerler".into(),
                    );
                }
                let mut words = vec![action.header()];
                for operand in action.operands() {
                    match operand {
                        crate::dmv_comm::DmvOperand::Word(device) => {
                            words.extend(data_destination(device)?)
                        }
                        crate::dmv_comm::DmvOperand::Bit(device) => {
                            words.extend(bit_operand(internal_bit_address(device)?))
                        }
                    }
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::Ethernet(action) => {
                if !self.local_layout {
                    return Err(
                        "ETHRW: native ES2/SS2 gövdesi 9 yerine yalnız 1 word ilerler".into(),
                    );
                }
                let mut words = vec![action.header()];
                for operand in action.operands() {
                    match operand {
                        crate::serial_io::SerialOperand::Value(value) => {
                            words.extend(calendar_operand(&value)?)
                        }
                        crate::serial_io::SerialOperand::Data(device) => {
                            words.extend(data_destination(device)?)
                        }
                    }
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::RawEthernet(action) => {
                if !self.local_layout {
                    return Err("ETHRS: kurulu ES2/SS2 dispatcher bu komutu çalıştırmıyor".into());
                }
                let mut words = vec![action.header()];
                for operand in action.operands() {
                    match operand {
                        crate::ethernet_raw::RawEthernetOperand::Value(value) => {
                            words.extend(calendar_operand(&value)?)
                        }
                        crate::ethernet_raw::RawEthernetOperand::Word(device) => {
                            words.extend(data_destination(device)?)
                        }
                        crate::ethernet_raw::RawEthernetOperand::Bit(device) => {
                            words.extend(bit_operand(internal_bit_address(device)?))
                        }
                    }
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::Pu(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: yeni Delta editor kodlaması biliniyor; kurulu eski ES2/SS2 simülatöründe PU desteği yok",
                        action.name()
                    ));
                }
                let Some(header) = action.header() else {
                    return self.reserve_local_instruction(
                        action.words(),
                        "PU: no recovered compiler opcode",
                    );
                };
                let mut words = vec![header];
                for operand in action.operands() {
                    match operand {
                        crate::pu_module::PuOperand::Value(value) => {
                            words.extend(calendar_operand_at_width(&value, action.wide())?)
                        }
                        crate::pu_module::PuOperand::Input(device)
                        | crate::pu_module::PuOperand::Data(device)
                        | crate::pu_module::PuOperand::Bit(device) => {
                            if device.bit() {
                                words.extend(bit_operand(internal_bit_address(device)?));
                            } else {
                                words.extend(data_destination(device)?);
                            }
                            if action.wide() {
                                words.extend([0x0080, 0x0080]);
                            }
                        }
                    }
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::CoordinatedMotion(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 hareket davranışı ve PC ilerlemesi PDF'den farklı; genel karşılaştırmalı yükleme açık değil",
                        action.name()
                    ));
                }
                let Some(header) = action.header() else {
                    return self.reserve_local_instruction(
                        action.words(),
                        "Coordinated motion: no recovered native opcode",
                    );
                };
                let mut words = vec![header];
                for operand in action.operands() {
                    match operand {
                        crate::coordinated_motion::CoordinatedOperand::Value(value) => {
                            words.extend(calendar_operand_at_width(&value, action.wide())?)
                        }
                        crate::coordinated_motion::CoordinatedOperand::Device(device) => {
                            if device.bit() {
                                words.extend(bit_operand(internal_bit_address(device)?));
                            } else {
                                words.extend(data_destination(device)?);
                            }
                            if action.wide() {
                                words.extend([0x0080, 0x0080]);
                            }
                        }
                    }
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::CanExchange(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 dispatcher bu komutu çalıştırmıyor",
                        action.name()
                    ));
                }
                let Some(header) = action.header() else {
                    return self.reserve_local_instruction(
                        action.words(),
                        "COPWL: documented length without a recovered WPL opcode",
                    );
                };
                let mut words = vec![header];
                for operand in action.operands() {
                    match operand {
                        crate::canopen_exchange::ExchangeOperand::Value(value) => {
                            words.extend(calendar_operand_at_width(&value, action.wide())?)
                        }
                        crate::canopen_exchange::ExchangeOperand::Device(device)
                            if device.bit() =>
                        {
                            words.extend(bit_operand(internal_bit_address(device)?));
                            if action.wide() {
                                words.extend([0x0080, 0x0080]);
                            }
                        }
                        crate::canopen_exchange::ExchangeOperand::Device(device) => {
                            words.extend(data_destination(device)?);
                            if action.wide() {
                                words.extend([0x0080, 0x0080]);
                            }
                        }
                    }
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::Canopen(action) => {
                if !self.local_layout {
                    return Err(
                        "CANopen: kurulu ES2/SS2 dispatcher bu komutları çalıştırmıyor".into(),
                    );
                }
                let Some(header) = action.header() else {
                    return self.reserve_local_instruction(
                        action.words(),
                        "CANopen: documented length without a recovered WPL opcode",
                    );
                };
                let mut words = vec![header];
                for operand in action.operands() {
                    match operand {
                        crate::serial_io::SerialOperand::Value(value) => {
                            words.extend(calendar_operand(&value)?)
                        }
                        crate::serial_io::SerialOperand::Data(device) => {
                            if device.bit() {
                                words.extend(bit_operand(internal_bit_address(device)?));
                            } else {
                                words.extend(data_destination(device)?);
                            }
                        }
                    }
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::Servo(action) => {
                if !self.local_layout {
                    return Err("ASDRW: kurulu ES2/SS2 yordamı 7 word yerine yalnız 1 word ilerliyor; servo karşılaştırmalı yükleme desteklenmiyor".into());
                }
                let mut words = vec![action.header()];
                for operand in action.operands() {
                    match operand {
                        crate::serial_io::SerialOperand::Value(value) => {
                            words.extend(calendar_operand(&value)?)
                        }
                        crate::serial_io::SerialOperand::Data(device) => {
                            words.extend(data_destination(device)?)
                        }
                    }
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::Drive(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 yordamı yalnız program sayacını ilerletiyor; sürücü haberleşmesi işlevsel karşılaştırması yok",
                        action.name()
                    ));
                }
                let mut words = vec![action.header()];
                for operand in action.operands() {
                    words.extend(calendar_operand(&operand)?);
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::AnalogKnob(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 yordamı yalnız program sayacını ilerletiyor; analog ayar düğmesi işlevsel karşılaştırması yok",
                        action.name()
                    ));
                }
                let mut words = vec![action.header()];
                for operand in action.operands() {
                    words.extend(additional_operand(&operand)?);
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::SerialIo(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 yordamı yalnız program sayacını ilerletiyor; seri haberleşme için işlevsel karşılaştırmalı yükleme yok",
                        action.name()
                    ));
                }
                use crate::serial_io::SerialOperand;
                let mut words = vec![action.header()];
                for operand in action.operands() {
                    match operand {
                        SerialOperand::Value(value) => words.extend(calendar_operand(&value)?),
                        SerialOperand::Data(device) => words.extend(data_destination(device)?),
                    }
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::ModuleIo(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 yordamı yalnız program sayacını ilerletiyor; modül register işlemleri için işlevsel karşılaştırmalı yükleme yok",
                        action.name()
                    ));
                }
                let mut words = vec![action.header()];
                for operand in action.operands() {
                    words.extend(calendar_operand(&operand)?);
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::MotionBasic(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 konum modeli bilgisayar saatine bağlıdır; DZRN fiziksel referans girişlerini izlemez, DABSR program sayacı kusurlu, DICF yordamı yalnız atlar. Belgeli sanal hareket modeliyle otomatik eşitlik karşılaştırması desteklenmiyor",
                        action.name()
                    ));
                }
                use crate::motion_basic::MotionOperand;
                let mut words = vec![action.header()];
                for operand in action.operands() {
                    match operand {
                        MotionOperand::Value(value) => words.extend(calendar_operand(&value)?),
                        MotionOperand::Bit(device) => {
                            words.extend(bit_operand(internal_bit_address(device)?));
                            if action.wide() {
                                words.extend([0x0080, 0x0080]);
                            }
                        }
                    }
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::ExternalIo(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 yordamı yalnız program sayacını ilerletiyor; sanal klavye/gösterge/ASCII çıkışı için işlevsel karşılaştırmalı yükleme yok",
                        action.name()
                    ));
                }
                use crate::external_io::ExternalIoAction;
                let bit = |device| -> Result<Vec<u16>, String> {
                    let mut value = bit_operand(internal_bit_address(device)?).to_vec();
                    if action.wide() {
                        value.extend([0x0080, 0x0080]);
                    }
                    Ok(value)
                };
                let mut words = vec![action.header()];
                match action {
                    ExternalIoAction::TenKey {
                        source,
                        value,
                        status,
                        ..
                    } => {
                        words.extend(bit(*source)?);
                        words.extend(calendar_operand(value)?);
                        words.extend(bit(*status)?);
                    }
                    ExternalIoAction::HexKey {
                        source,
                        strobe,
                        value,
                        status,
                        ..
                    } => {
                        words.extend(bit(*source)?);
                        words.extend(bit(*strobe)?);
                        words.extend(calendar_operand(value)?);
                        words.extend(bit(*status)?);
                    }
                    ExternalIoAction::Dip {
                        source,
                        strobe,
                        value,
                        groups,
                    } => {
                        words.extend(bit(*source)?);
                        words.extend(bit(*strobe)?);
                        words.extend(numeric_operand(&Operand::Register(*value))?);
                        words.extend(numeric_operand(&Operand::Constant(i16::from(*groups)))?);
                    }
                    ExternalIoAction::Segment {
                        source,
                        output,
                        mode,
                    } => {
                        words.extend(calendar_operand(source)?);
                        words.extend(bit(*output)?);
                        words.extend(numeric_operand(&Operand::Constant(i16::from(*mode)))?);
                    }
                    ExternalIoAction::Arrows {
                        source,
                        value,
                        output,
                        mode,
                    } => {
                        words.extend(bit(*source)?);
                        words.extend(calendar_operand(value)?);
                        words.extend(bit(*output)?);
                        words.extend(numeric_operand(&Operand::Constant(i16::from(*mode)))?);
                    }
                    ExternalIoAction::Print { source, output } => {
                        words.extend(numeric_operand(&Operand::Register(*source))?);
                        words.extend(bit(*output)?);
                    }
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::PulseOutput(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 darbe modeli gerçek bilgisayar saatini kullanır; PLSR rampayı işlemez, PWM yordamı çıkış üretmez. Belgeli sanal zaman modeliyle otomatik eşitlik karşılaştırması desteklenmiyor",
                        action.name()
                    ));
                }
                // Genuine compiler widths/headers define instruction PCs;
                // these bytes do not imply simulator/hardware timing parity.
                use crate::pulse_output::PulseAction;
                let mut words = vec![action.header()];
                match action {
                    PulseAction::Train {
                        frequency, count, ..
                    } => {
                        words.extend(calendar_operand(frequency)?);
                        words.extend(calendar_operand(count)?);
                    }
                    PulseAction::Pwm { width, period, .. } => {
                        words.extend(calendar_operand(width)?);
                        words.extend(calendar_operand(period)?);
                    }
                    PulseAction::Ramp {
                        frequency,
                        count,
                        ramp,
                        ..
                    } => {
                        words.extend(calendar_operand(frequency)?);
                        words.extend(calendar_operand(count)?);
                        words.extend(calendar_operand(ramp)?);
                    }
                }
                words.extend(bit_operand(internal_bit_address(action.dest())?));
                if action.wide() {
                    words.extend([0x0080, 0x0080]);
                }
                debug_assert_eq!(words.len(), usize::from(action.words()));
                self.instruction(&words)
            }
            Action::HighSpeed(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 yordamı hesaplama yapmıyor; sanal yüksek hızlı olay profili ayrı doğrulama gerektirir",
                        action.name()
                    ));
                }
                self.reserve_local_instruction(
                    action.words(),
                    "High-speed action: local instruction span only",
                )
            }
            Action::DoubleCounter(action) => {
                use crate::counter::CounterPreset;
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::DoubleCounter(self.output.action_pc.len() - 1),
                );
                if action.counter.area != Area::C || !(200..=254).contains(&action.counter.index) {
                    return Err("DCNT hedefi C200..C254 olmalı".into());
                }
                if !self.local_layout && action.counter.index >= 235 {
                    return Err("Native DCNT yalnız doğrulanmış C200..C234 coil yoluna yüklenebilir; C235..C254 fiziksel sayaç yolu henüz doğrulanmadı".into());
                }
                let preset = match action.preset {
                    CounterPreset::Constant(value) => wide_operand(&WideOperand::Constant(value))?,
                    CounterPreset::Data(device) => {
                        if device.area != Area::D {
                            return Err("DCNT ayarı D çifti olmalı".into());
                        }
                        crate::wide::double_devices(device)?;
                        wide_operand(&WideOperand::Register(device))?
                    }
                };
                // Genuine special coil descriptor uses the logical C number,
                // not the natural counter byte-offset descriptor used by DMOV.
                // WPL selector4 emits tag83. Earlier runtime captures used80,
                // which the interpreter tolerates but is not compiler output.
                self.instruction(&[
                    0x0600,
                    (action.counter.index << 8) | 0x0083,
                    preset[0],
                    preset[1],
                    preset[2],
                    preset[3],
                ])
            }
            Action::EthernetConfig(action) => {
                if !self.local_layout {
                    return Err("ETHCONF/ETHMSG: SE2 kullanıcı soketleri kurulu ES2/SS2 simülatöründe doğrulanmadı".into());
                }
                let mut words = vec![action.header().ok_or("Ethernet opcode eksik")?];
                for arg in action.operands() {
                    match arg {
                        crate::ethernet_config::EthernetConfigOperand::Value(v) => {
                            words.extend(calendar_operand(&v)?)
                        }
                        crate::ethernet_config::EthernetConfigOperand::Word(v) => {
                            words.extend(data_destination(v)?)
                        }
                        crate::ethernet_config::EthernetConfigOperand::Bit(v) => {
                            words.extend(bit_operand(internal_bit_address(v)?))
                        }
                    }
                }
                if words.len() != usize::from(action.words()) {
                    return Err("Ethernet word uzunluğu uyuşmuyor".into());
                }
                self.instruction(&words)
            }
            Action::Ftc(action) => {
                if !self.local_layout {
                    return Err(
                        "FTC: kurulu ES2/SS2 sayısal hesap yapmıyor; native yükleme kapalı".into(),
                    );
                }
                let mut words = vec![0x4201];
                words.extend(numeric_operand(&action.setpoint)?);
                words.extend(numeric_operand(&action.process)?);
                words.extend(data_destination(action.parameters)?);
                words.extend(data_destination(action.output)?);
                self.instruction(&words)
            }
            Action::Pid(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu Delta editor ES2/SS2 yordamı hesaplama yapmıyor; belgeli yerel PID profili özgün simülatöre yüklenemez",
                        action.name()
                    ));
                }
                self.reserve_local_instruction(
                    if action.wide { 17 } else { 9 },
                    "PID: local instruction span only",
                )
            }
            Action::Station(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 simülatöründe işlevsel istasyon yordamı yok; yerel istasyon profili kullanın",
                        action.name()
                    ));
                }
                self.reserve_local_instruction(
                    action.layout_words(),
                    "Station action: local instruction span only",
                )
            }
            Action::StringMove(action) => {
                if !self.local_layout {
                    return Err("$MOV: kurulu Delta editor komut tablosunda kodlaması bulunmuyor; yeni firmware komutu yalnız yerel profilde kullanılabilir".into());
                }
                self.reserve_local_instruction(
                    action.layout_words(),
                    "$MOV: no recovered native encoding",
                )
            }
            Action::Timing(action) => {
                if !self.local_layout {
                    return Err(format!(
                        "{}: kurulu ES2/SS2 başlık adayı işlevsiz; sanal zamanlama/G/Ç profili native eşdeğerlik olarak yüklenemez",
                        action.name()
                    ));
                }
                use crate::timing_io::TimingAction;
                let words = match action {
                    TimingAction::Delay { duration } => {
                        let mut words = vec![0x3e01];
                        words.extend(numeric_operand(duration)?);
                        words
                    }
                    TimingAction::Pwm { width, cycle, dest } => {
                        let mut words = vec![0x4001];
                        words.extend(numeric_operand(&Operand::Register(*width))?);
                        words.extend(numeric_operand(&Operand::Register(*cycle))?);
                        words.extend(bit_operand(internal_bit_address(*dest)?));
                        words
                    }
                    TimingAction::CatchSpeed {
                        input,
                        sample,
                        output,
                    } => {
                        let mut words = vec![0xbc01];
                        words.extend(bit_operand(internal_bit_address(*input)?));
                        words.extend(numeric_operand(&Operand::Register(*sample))?);
                        words.extend(numeric_operand(&Operand::Register(*output))?);
                        words
                    }
                };
                self.instruction(&words)
            }
            Action::Sfc(crate::sfc::SfcAction::Ist {
                source,
                first,
                last,
            }) => {
                let mut words = vec![0x8800];
                for d in [source, first, last] {
                    words.extend(bit_operand(internal_bit_address(*d)?));
                }
                self.instruction(&words)
            }
            Action::Alarm(action) => {
                let mut words = vec![action.header()];
                if let crate::alarm::AlarmAction::Set {
                    timer,
                    preset,
                    alarm,
                } = action
                {
                    words.extend(numeric_operand(&Operand::Register(*timer))?);
                    words.extend(numeric_operand(&Operand::Constant(*preset as i16))?);
                    words.extend(bit_operand(internal_bit_address(*alarm)?));
                }
                self.instruction(&words)
            }
            Action::SpecialTimer(action) => {
                use crate::special_timers::{TimerAction, TimerPreset};
                let preset = |p: &TimerPreset| {
                    numeric_operand(&match p {
                        TimerPreset::Constant(v) => Operand::Constant(*v),
                        TimerPreset::Register(d) => Operand::Register(*d),
                    })
                };
                let mut words = Vec::new();
                match action {
                    TimerAction::Timer {
                        timer,
                        preset: p,
                        contact,
                    } => {
                        if *contact {
                            words.push(0x1c02);
                            words.extend(numeric_operand(&Operand::Register(*timer))?);
                        } else {
                            words.extend([0x0300, (timer.index << 8) | 0x80]);
                        }
                        words.extend(preset(p)?);
                    }
                    TimerAction::Teach { dest, scale } => {
                        words.push(0x9000);
                        words.extend(data_destination(*dest)?);
                        words.extend(numeric_operand(&Operand::Constant(i16::from(*scale)))?);
                    }
                    TimerAction::Special {
                        timer,
                        preset: p,
                        dest,
                    } => {
                        words.push(0x9200);
                        words.extend(numeric_operand(&Operand::Register(*timer))?);
                        words.extend(numeric_operand(&Operand::Constant(*p as i16))?);
                        words.extend(bit_operand(internal_bit_address(*dest)?));
                    }
                }
                self.instruction(&words)
            }
            Action::Matrix(action) => {
                use crate::matrix_ops::MatrixAction;
                if !self.local_layout && action.installed_unsafe_natural_start() {
                    return Err("MAND/MOR/MXOR/MXNR: doğrudan C200+ başlangıcı kurulu native yordamda kilitlenme/durum bozulması oluşturuyor; C199 üzerinden doğrulanmış ham geçiş ayrı kapsamdır".into());
                }
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::Matrix(self.output.action_pc.len() - 1),
                );
                let mut words = vec![action.header()];
                match action {
                    MatrixAction::Binary {
                        left,
                        right,
                        dest,
                        count,
                        ..
                    } => {
                        words.extend(matrix_operand(left, self.local_layout)?);
                        words.extend(matrix_operand(right, self.local_layout)?);
                        words.extend(matrix_operand(dest, self.local_layout)?);
                        words.extend(data_value_operand(count)?);
                    }
                    MatrixAction::Invert {
                        source,
                        dest,
                        count,
                    }
                    | MatrixAction::Shift {
                        source,
                        dest,
                        count,
                    }
                    | MatrixAction::Rotate {
                        source,
                        dest,
                        count,
                    } => {
                        words.extend(matrix_operand(source, self.local_layout)?);
                        words.extend(matrix_operand(dest, self.local_layout)?);
                        words.extend(data_value_operand(count)?);
                    }
                    MatrixAction::Compare {
                        left,
                        right,
                        count,
                        pointer,
                    } => {
                        words.extend(matrix_operand(left, self.local_layout)?);
                        words.extend(matrix_operand(right, self.local_layout)?);
                        words.extend(data_value_operand(count)?);
                        words.extend(matrix_scalar_operand(pointer)?);
                    }
                    MatrixAction::Read {
                        source,
                        count,
                        pointer,
                    }
                    | MatrixAction::Write {
                        source,
                        count,
                        pointer,
                    } => {
                        words.extend(matrix_operand(source, self.local_layout)?);
                        words.extend(data_value_operand(count)?);
                        words.extend(matrix_scalar_operand(pointer)?);
                    }
                    MatrixAction::Count {
                        source,
                        count,
                        dest,
                    } => {
                        words.extend(matrix_operand(source, self.local_layout)?);
                        words.extend(data_value_operand(count)?);
                        words.extend(matrix_scalar_operand(dest)?);
                    }
                }
                self.instruction(&words)
            }
            Action::FileMemory(action) => {
                if !self.local_layout {
                    return Err("MEMR/MEMW: kurulu ES2/SS2 yordamları yalnız PC ilerletir; açık kalıcı dosya belleği backend'i kullanılmalı, native eşdeğerlik doğrulanamaz".into());
                }
                use crate::file_memory::FileAction;
                let mut words = vec![action.header()];
                match action {
                    FileAction::Read {
                        wide,
                        index,
                        dest,
                        count,
                    } => {
                        words.extend(data_value_operand(index)?);
                        if *wide {
                            words.extend(wide_operand(&WideOperand::Register(*dest))?);
                        } else {
                            words.extend(numeric_operand(&Operand::Register(*dest))?);
                        }
                        words.extend(data_value_operand(count)?);
                    }
                    FileAction::Write {
                        wide,
                        source,
                        index,
                        count,
                    } => {
                        if *wide {
                            words.extend(wide_operand(&WideOperand::Register(*source))?);
                        } else {
                            words.extend(numeric_operand(&Operand::Register(*source))?);
                        }
                        words.extend(data_value_operand(index)?);
                        words.extend(data_value_operand(count)?);
                    }
                }
                self.instruction(&words)
            }
            Action::Drum(action) => {
                use crate::drum::DrumAction;
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::Drum(self.output.action_pc.len() - 1),
                );
                let mut words = vec![action.header()];
                words.extend(drum_operand(action.source(), self.local_layout)?);
                match action {
                    DrumAction::Absolute { value, .. } => {
                        words.extend(drum_operand(value, self.local_layout)?)
                    }
                    DrumAction::Incremental { counter, .. } => {
                        words.extend(numeric_operand(&Operand::Register(*counter))?)
                    }
                }
                words.extend(bit_operand(internal_bit_address(action.dest())?));
                if action.wide() {
                    words.extend([0x0080, 0x0080]);
                    words.extend(wide_operand(&WideOperand::Constant(i32::from(
                        action.count(),
                    )))?);
                } else {
                    words.extend(numeric_operand(&Operand::Constant(action.count() as i16))?);
                }
                self.instruction(&words)
            }
            Action::Hour(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::CalendarClock(self.output.action_pc.len() - 1),
                );
                let mut words = vec![action.header()];
                words.extend(calendar_operand(&action.source)?);
                if action.wide() {
                    words.extend(wide_operand(&WideOperand::Register(action.hours))?);
                    // Installed wide instructions use a full 8-byte descriptor
                    // even for the single destination bit.
                    words.extend(bit_operand(internal_bit_address(action.dest)?));
                    words.extend([0x0080, 0x0080]);
                } else {
                    words.extend(numeric_operand(&Operand::Register(action.hours))?);
                    words.extend(bit_operand(internal_bit_address(action.dest)?));
                }
                self.instruction(&words)
            }
            Action::Rtc(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedRtc(self.output.action_pc.len() - 1),
                );
                self.instruction(
                    &crate::indexed_rtc::IndexedRtcAction::from_legacy(action).native_words()?,
                )
            }
            Action::Time(action) => {
                let words = action.native_words()?;
                if matches!(action, crate::time_ops::TimeAction::Arithmetic { .. }) {
                    self.output.preflight.insert(
                        self.output.next_pc.len(),
                        PreflightCheck::IndexedCalendarTime(self.output.action_pc.len() - 1),
                    );
                    self.time_arithmetic_instruction(&words)
                } else {
                    self.instruction(&words)
                }
            }
            Action::Gps(action) => {
                if !self.local_layout {
                    return Err("GPS: kurulu ES2/SS2 yordam adayı NMEA alımı yapmıyor; açıkça yapılandırılmış sanal COM1 arka ucunu kullanın".into());
                }
                let mut words = vec![0x8001];
                words.extend(numeric_operand(&action.source)?);
                words.extend(numeric_operand(&Operand::Register(action.dest))?);
                self.instruction(&words)
            }
            Action::Solar(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::Solar(self.output.action_pc.len() - 1),
                );
                let mut words = vec![action.header()];
                words.extend(wide_operand(&WideOperand::Register(action.source))?);
                words.extend(wide_operand(&WideOperand::Register(action.dest))?);
                self.instruction(&words)
            }
            Action::Search(action) => {
                use crate::search_ops::SearchAction;
                let mut words = vec![action.header()];
                match action {
                    SearchAction::Toggle { dest } => {
                        words.extend(handler_default_bit_operand(*dest)?)
                    }
                    SearchAction::Search {
                        source,
                        value,
                        dest,
                        count,
                    } => {
                        self.output.preflight.insert(
                            self.output.next_pc.len(),
                            PreflightCheck::Search(self.output.action_pc.len() - 1),
                        );
                        words.extend(additional_operand(source)?);
                        words.extend(calendar_operand(value)?);
                        words.extend(additional_operand(dest)?);
                        words.extend(data_value_operand(count)?);
                    }
                }
                self.instruction(&words)
            }
            Action::RampSort(action) => {
                use crate::ramp_sort::RampSortAction;
                let wide = action.wide();
                let device = |d| {
                    data_value_operand(&if wide {
                        crate::data_processing::DataValue::Double(WideOperand::Register(d))
                    } else {
                        crate::data_processing::DataValue::Word(Operand::Register(d))
                    })
                };
                let constant = |v: u8| {
                    data_value_operand(&if wide {
                        crate::data_processing::DataValue::Double(WideOperand::Constant(i32::from(
                            v,
                        )))
                    } else {
                        crate::data_processing::DataValue::Word(Operand::Constant(i16::from(v)))
                    })
                };
                let mut words = vec![action.header()];
                match action {
                    RampSortAction::Ramp {
                        start,
                        end,
                        dest,
                        count,
                        ..
                    } => {
                        for d in [start, end, dest] {
                            words.extend(device(*d)?);
                        }
                        words.extend(data_value_operand(count)?);
                    }
                    RampSortAction::Sort {
                        source,
                        rows,
                        columns,
                        dest,
                        count,
                        ..
                    } => {
                        self.output.preflight.insert(
                            self.output.next_pc.len(),
                            PreflightCheck::Sort(self.output.action_pc.len() - 1),
                        );
                        words.extend(device(*source)?);
                        words.extend(constant(*rows)?);
                        words.extend(constant(*columns)?);
                        words.extend(device(*dest)?);
                        words.extend(data_value_operand(count)?);
                    }
                }
                self.instruction(&words)
            }
            Action::Transform(action) => {
                if !self.local_layout
                    && action
                        .mode
                        .constant()
                        .is_some_and(|m| !self.transform_profile.supports(m))
                {
                    return Err("DTM: seçilen modun yerel davranışı kurulu ES2/SS2 yordamında doğrulanmıyor; yerel simülasyonu kullanın".into());
                }
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::Transform(self.output.action_pc.len() - 1),
                );
                let mut words = vec![action.header()];
                words.extend(numeric_operand(&Operand::Register(action.source))?);
                words.extend(numeric_operand(&Operand::Register(action.dest))?);
                words.extend(data_value_operand(&action.mode)?);
                words.extend(data_value_operand(&action.count)?);
                self.instruction(&words)
            }
            Action::Additional(
                legacy @ (crate::additional_processing::AdditionalAction::WordSum { .. }
                | crate::additional_processing::AdditionalAction::Scale { .. }
                | crate::additional_processing::AdditionalAction::ScalePoints { .. }
                | crate::additional_processing::AdditionalAction::CompareTable { .. }),
            ) => {
                let action =
                    crate::indexed_additional::IndexedAdditionalAction::from_legacy(legacy)?;
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::IndexedAdditional(self.output.action_pc.len() - 1),
                );
                self.instruction(&action.native_words()?)
            }
            Action::Additional(action) => {
                use crate::additional_processing::{AdditionalAction, AdditionalValue};
                let wide = action.wide();
                let device = |d, width| {
                    additional_operand(&if width {
                        AdditionalValue::Double(WideOperand::Register(d))
                    } else {
                        AdditionalValue::Word(Operand::Register(d))
                    })
                };
                let value = |v: &crate::data_processing::DataValue| {
                    additional_operand(&match v {
                        crate::data_processing::DataValue::Word(v) => {
                            AdditionalValue::Word(v.clone())
                        }
                        crate::data_processing::DataValue::Double(v) => {
                            AdditionalValue::Double(v.clone())
                        }
                    })
                };
                let mut words = vec![action.header()];
                match action {
                    AdditionalAction::Swap { dest, .. } => {
                        words.extend(calendar_operand(dest.value())?)
                    }
                    AdditionalAction::MaskMove { source, mask, dest } => {
                        words.extend(bit_index_operand(source)?);
                        words.extend(bit_index_operand(mask)?);
                        words.extend(mask_random_destination_operand(dest)?);
                    }
                    AdditionalAction::SignExtend { source, dest } => {
                        words.extend(additional_operand(source)?);
                        // Word instruction fields, signed word input and a
                        // 32-bit result writer. C200+ needs its natural-counter
                        // address descriptor; only its first two words are encoded.
                        words.extend(&wide_operand(&WideOperand::Register(*dest))?[..2]);
                    }
                    AdditionalAction::WordSum {
                        source,
                        count,
                        dest,
                        ..
                    } => {
                        words.extend(device(*source, wide)?);
                        words.extend(value(count)?);
                        if !wide && dest.area == Area::C && dest.index >= 200 {
                            words.extend(&wide_operand(&WideOperand::Register(*dest))?[..2]);
                        } else {
                            words.extend(device(*dest, wide)?);
                        }
                    }
                    AdditionalAction::Scale {
                        source,
                        slope,
                        offset,
                        dest,
                    } => {
                        for v in [source, slope, offset] {
                            words.extend(numeric_operand(v)?);
                        }
                        words.extend(device(*dest, false)?);
                    }
                    AdditionalAction::ScalePoints {
                        source,
                        params,
                        dest,
                        ..
                    } => {
                        words.extend(value(source)?);
                        words.extend(device(*params, wide)?);
                        words.extend(device(*dest, wide)?);
                    }
                    AdditionalAction::CompareTable {
                        left,
                        right,
                        count,
                        dest,
                        ..
                    } => {
                        words.extend(device(*left, wide)?);
                        words.extend(device(*right, wide)?);
                        words.extend(value(count)?);
                        words.extend(additional_operand(dest)?);
                    }
                }
                self.instruction(&words)
            }
            Action::Random(action) => {
                let words = action.native_words()?;
                if action.wide() {
                    self.drand_instruction(&words)
                } else {
                    self.instruction(&words)
                }
            }
            Action::Float(action) => {
                use crate::float_ops::FloatAction;
                let mut words = vec![match action {
                    FloatAction::Advanced(action) => action.op.header(),
                    FloatAction::Math(action) => action.op.header(),
                    FloatAction::Float { wide: false, .. } => 0x7200,
                    FloatAction::Float { wide: true, .. } => 0x7300,
                    FloatAction::Integer { wide: false, .. } => 0x2201,
                    FloatAction::Integer { wide: true, .. } => 0x2301,
                    FloatAction::Move { .. } => 0xf100,
                    FloatAction::Compare { .. } => 0xed00,
                    FloatAction::Zone { .. } => 0xef00,
                }];
                match action {
                    FloatAction::Advanced(action) => {
                        words.extend(float_operand(&action.source)?);
                        if let Some(right) = &action.right {
                            words.extend(float_operand(right)?);
                        }
                        words.extend(wide_operand(&WideOperand::Register(action.dest))?);
                    }
                    FloatAction::Math(action) => {
                        words.extend(float_operand(&action.source)?);
                        if let Some(right) = &action.right {
                            words.extend(float_operand(right)?);
                        }
                        words.extend(wide_operand(&WideOperand::Register(action.dest))?);
                    }
                    FloatAction::Float { source, dest, wide }
                    | FloatAction::Integer { source, dest, wide } => {
                        for d in [source, dest] {
                            if *wide {
                                words.extend(wide_operand(&WideOperand::Register(*d))?);
                            } else if matches!(action, FloatAction::Integer { .. }) && d == source {
                                // INT has a two-word source field, but reads a
                                // binary32 value, including a natural counter.
                                words.extend(&wide_operand(&WideOperand::Register(*d))?[..2]);
                            } else {
                                words.extend(numeric_operand(&Operand::Register(*d))?);
                            }
                        }
                    }
                    FloatAction::Move { source, dest } => {
                        words.extend(float_operand(source)?);
                        words.extend(additional_operand(dest)?);
                    }
                    FloatAction::Compare { left, right, dest } => {
                        words.extend(float_operand(left)?);
                        words.extend(float_operand(right)?);
                        words.extend(comparison_result_operand(*dest)?);
                        words.extend([0x0080, 0x0080]);
                    }
                    FloatAction::Zone {
                        lower,
                        upper,
                        value,
                        dest,
                    } => {
                        for operand in [lower, upper, value] {
                            words.extend(float_operand(operand)?);
                        }
                        words.extend(comparison_result_operand(*dest)?);
                        words.extend([0x0080, 0x0080]);
                    }
                }
                let index = self.output.next_pc.len();
                self.instruction(&words)?;
                if matches!(action, FloatAction::Math(a) if a.op == crate::float_math::MathOp::DivideReal)
                {
                    let target = self.output.words.len() as u16 + 4;
                    self.output.padded_error_skip.insert(index, target);
                    for _ in 0..4 {
                        self.instruction(&[0xfdf0])?;
                    }
                }
                Ok(())
            }
            Action::Text(action) => {
                use crate::text_encoding::TextAction;
                if matches!(action, TextAction::Lrc { .. } | TextAction::Crc { .. }) {
                    self.output.preflight.insert(
                        self.output.next_pc.len(),
                        PreflightCheck::IndexedChecksum(self.output.action_pc.len() - 1),
                    );
                }
                if matches!(
                    action,
                    TextAction::Ascii { .. } | TextAction::Hex { .. } | TextAction::Check { .. }
                ) {
                    self.output.preflight.insert(
                        self.output.next_pc.len(),
                        PreflightCheck::IndexedText(self.output.action_pc.len() - 1),
                    );
                }
                if self.local_layout && matches!(action, TextAction::Literal { .. }) {
                    return self.reserve_local_instruction(11, "ASC: local instruction span only");
                }
                let mut words = vec![match action {
                    TextAction::Segment { .. } => 0xa200,
                    TextAction::Literal { .. } => return Err("ASC: kurulu ES2/SS2 simülatöründeki yordam yalnız PC ilerletir; bu komut yerel simülasyonda kullanılabilir".into()),
                    TextAction::Ascii { .. } => 0xb400,
                    TextAction::Hex { .. } => 0xb600,
                    TextAction::Check { .. } => 0xb800,
                    TextAction::Lrc { .. } => 0xd000,
                    TextAction::Crc { .. } => 0xd200,
                }];
                match action {
                    TextAction::Segment { source, dest } => {
                        words.extend(calendar_operand(source)?);
                        words.extend(calendar_operand(dest.value())?);
                    }
                    TextAction::Ascii {
                        source,
                        dest,
                        count,
                    }
                    | TextAction::Hex {
                        source,
                        dest,
                        count,
                    } => {
                        words.extend(additional_operand(source.value())?);
                        words.extend(additional_operand(dest.value())?);
                        words.extend(numeric_operand(&Operand::Constant(*count as i16))?);
                    }
                    TextAction::Check {
                        source,
                        dest,
                        count,
                    } => {
                        words.extend(additional_operand(source.value())?);
                        words.extend(additional_operand(dest.value())?);
                        words.extend(numeric_operand(count)?);
                    }
                    TextAction::Lrc {
                        source,
                        dest,
                        count,
                    }
                    | TextAction::Crc {
                        source,
                        dest,
                        count,
                    } => {
                        words.extend(numeric_operand(&Operand::Register(*source))?);
                        words.extend(numeric_operand(count)?);
                        words.extend(numeric_operand(&Operand::Register(*dest))?);
                    }
                    TextAction::Literal { .. } => unreachable!(),
                }
                self.instruction(&words)
            }
            Action::Bits(action) => {
                use crate::bit_tools::BitAction;
                let mut words = vec![action.header()];
                match action {
                    BitAction::Absolute { dest } => words.extend(calendar_operand(dest.value())?),
                    BitAction::Gray { source, dest, .. } => {
                        words.extend(calendar_operand(source)?);
                        words.extend(calendar_operand(dest.value())?);
                    }
                    BitAction::IndexedWrite { dest, index, .. } => {
                        words.extend(calendar_operand(dest.value())?);
                        words.extend(bit_index_operand(index)?);
                    }
                }
                self.instruction(&words)
            }
            Action::Data(data) => {
                use crate::data_processing::{
                    DataAction, DecodeDestination, DecodeSource, MeanSource,
                };
                let device = |d: Device, wide: bool| -> Result<Vec<u16>, String> {
                    if matches!(d.area, Area::X | Area::Y | Area::M | Area::S) {
                        let mut pair = handler_default_bit_operand(d)?.to_vec();
                        if wide {
                            pair.extend([0x0080, 0x0080]);
                        }
                        Ok(pair)
                    } else if wide {
                        Ok(wide_operand(&WideOperand::Register(d))?.to_vec())
                    } else {
                        Ok(numeric_operand(&Operand::Register(d))?.to_vec())
                    }
                };
                let mut words = vec![data.header()];
                match data {
                    DataAction::Decode { source, dest, n } => {
                        words.extend(match source {
                            DecodeSource::Bits(d) => device(*d, false)?,
                            DecodeSource::Value(v) => calendar_operand(v)?,
                        });
                        words.extend(match dest {
                            DecodeDestination::Bits(d) => device(*d, false)?,
                            DecodeDestination::Value(value) => calendar_operand(value.value())?,
                        });
                        words.extend(numeric_operand(&Operand::Constant(*n as i16))?);
                    }
                    DataAction::Encode { source, dest, n } => {
                        words.extend(match source {
                            DecodeSource::Bits(d) => device(*d, false)?,
                            DecodeSource::Value(value) => calendar_operand(value)?,
                        });
                        words.extend(calendar_operand(dest.value())?);
                        words.extend(numeric_operand(&Operand::Constant(*n as i16))?);
                    }
                    DataAction::Sum { source, dest } => {
                        words.extend(calendar_operand(source)?);
                        words.extend(calendar_operand(dest.value())?);
                    }
                    DataAction::Root { source, dest } => {
                        words.extend(data_value_operand(source)?);
                        words.extend(device(*dest, source.wide())?);
                    }
                    DataAction::BitOn { source, dest, n } => {
                        words.extend(calendar_operand(source)?);
                        words.extend(handler_default_bit_operand(*dest)?);
                        if source.wide() {
                            words.extend([0x0080, 0x0080]);
                        }
                        words.extend(bit_index_operand(n)?);
                    }
                    DataAction::Mean {
                        wide,
                        source,
                        dest,
                        n,
                    } => {
                        words.extend(match source {
                            MeanSource::Numeric(d) => device(*d, *wide)?,
                            MeanSource::Group(value) => additional_operand(value)?,
                        });
                        words.extend(calendar_operand(dest.value())?);
                        words.extend(bit_index_operand(n)?);
                    }
                }
                self.instruction(&words)
            }
            Action::EdgeOut(dest, rising) => {
                let dest = bit_operand(internal_bit_address(*dest)?);
                self.instruction(&[if *rising { 0x0701 } else { 0x0801 }, dest[0], dest[1]])
            }
            Action::Shift(a) => {
                let const16 = |n: u16| numeric_operand(&Operand::Constant(n as i16));
                let word = |d: Device| numeric_operand(&Operand::Register(d));
                let bit = |d: Device| internal_bit_address(d).map(bit_operand);
                let mut words = Vec::new();
                match a {
                    ShiftAction::Rotate { op, wide, dest, n } => {
                        let header = match op {
                            Rotation::Right => 0x4c00,
                            Rotation::Left => 0x4e00,
                            Rotation::CarryRight => 0x5000,
                            Rotation::CarryLeft => 0x5200,
                        };
                        words.push(header + if *wide { 0x100 } else { 0 });
                        words.extend(calendar_operand(dest.value())?);
                        if *wide {
                            words.extend(wide_operand(&WideOperand::Constant(i32::from(*n)))?);
                        } else {
                            words.extend(const16(*n)?);
                        }
                    }
                    ShiftAction::Bits {
                        left,
                        source,
                        dest,
                        total,
                        n,
                    } => {
                        words.push(if *left { 0x5600 } else { 0x5400 });
                        words.extend(bit(*source)?);
                        words.extend(bit(*dest)?);
                        words.extend(const16(*total)?);
                        words.extend(const16(*n)?);
                    }
                    ShiftAction::Words {
                        left,
                        source,
                        dest,
                        total,
                        n,
                    } => {
                        words.push(if *left { 0x5a00 } else { 0x5800 });
                        words.extend(additional_operand(source.value())?);
                        words.extend(additional_operand(dest.value())?);
                        words.extend(const16(*total)?);
                        words.extend(const16(*n)?);
                    }
                    ShiftAction::Push {
                        source,
                        dest,
                        total,
                    } => {
                        words.push(0x5c00);
                        words.extend(calendar_operand(source)?);
                        words.extend(additional_operand(dest.value())?);
                        words.extend(const16(*total)?);
                    }
                    ShiftAction::Pop {
                        source,
                        dest,
                        total,
                    } => {
                        words.push(0x5e00);
                        words.extend(additional_operand(source.value())?);
                        words.extend(calendar_operand(dest.value())?);
                        words.extend(const16(*total)?);
                    }
                    ShiftAction::Reset { start, end } => {
                        words.push(0x6000);
                        for d in [start, end] {
                            words.extend(if matches!(d.area, Area::T | Area::C) {
                                [
                                    (d.index << 8) | 0x80,
                                    if d.area == Area::T { 0x0082 } else { 0x0084 },
                                ]
                            } else if d.area == Area::D {
                                word(*d)?
                            } else {
                                bit(*d)?
                            });
                        }
                    }
                    ShiftAction::Digits {
                        source,
                        m1,
                        m2,
                        dest,
                        n,
                    } => {
                        words.push(0x2a00);
                        words.extend(calendar_operand(source)?);
                        words.extend(const16(*m1)?);
                        words.extend(const16(*m2)?);
                        words.extend(calendar_operand(dest.value())?);
                        words.extend(const16(*n)?);
                    }
                };
                self.instruction(&words)
            }
            Action::Wide(a) => {
                a.destination_devices()?;
                let mut words = vec![a.op.header()];
                for source in &a.sources {
                    words.extend(calendar_operand(source.value())?);
                }
                if matches!(a.op, WideOp::Compare | WideOp::Zone) {
                    words.extend(comparison_result_operand(a.dest())?);
                    words.extend([0x0080, 0x0080]);
                } else if let crate::wide::WideDestination::Numeric(target) = &a.dest {
                    words.extend(calendar_operand(target.value())?);
                } else if let crate::wide::WideDestination::Product(target) = &a.dest {
                    words.extend(multiply_result_operand(target)?);
                } else if let crate::wide::WideDestination::Block(target) = &a.dest {
                    words.extend(additional_operand(target.value())?);
                } else {
                    words.extend(wide_operand(&WideOperand::Register(a.dest()))?);
                }
                if a.op == WideOp::Fill {
                    words.extend(wide_operand(&WideOperand::Constant(i32::from(a.count)))?);
                }
                self.instruction(&words)
            }
            Action::Convert(op, value, dest) => {
                let value = calendar_operand(value)?;
                let dest = calendar_operand(dest.value())?;
                self.instruction(&[
                    match op {
                        WordConversion::Bcd => 0x3400,
                        WordConversion::Bin => 0x3600,
                    },
                    value[0],
                    value[1],
                    dest[0],
                    dest[1],
                ])
            }
            Action::BlockMove(source, dest, count) => {
                let source = additional_operand(source.value())?;
                let dest = additional_operand(dest.value())?;
                let count = numeric_operand(count)?;
                self.instruction(&[
                    0x2e00, source[0], source[1], dest[0], dest[1], count[0], count[1],
                ])
            }
            Action::Fill(value, dest, count) => {
                let value = calendar_operand(value)?;
                let dest = additional_operand(dest.value())?;
                let count = numeric_operand(&Operand::Constant(*count as i16))?;
                self.instruction(&[
                    0x3000, value[0], value[1], dest[0], dest[1], count[0], count[1],
                ])
            }
            Action::Exchange(source, dest) => {
                let source = calendar_operand(source.value())?;
                let dest = calendar_operand(dest.value())?;
                self.instruction(&[0x3200, source[0], source[1], dest[0], dest[1]])
            }
            Action::ZoneCompare(low, high, value, d) => {
                let low = calendar_operand(low)?;
                let high = calendar_operand(high)?;
                let value = calendar_operand(value)?;
                let dest = comparison_result_operand(*d)?;
                self.instruction(&[
                    0x2600, low[0], low[1], high[0], high[1], value[0], value[1], dest[0], dest[1],
                ])
            }
            Action::MultiplyTruncated(a, b, d) | Action::DivideQuotient(a, b, d) => {
                let a = calendar_operand(a)?;
                let b = calendar_operand(b)?;
                let d = calendar_operand(d.value())?;
                self.instruction(&[
                    if matches!(action, Action::MultiplyTruncated(..)) {
                        0xf400
                    } else {
                        0xf600
                    },
                    a[0],
                    a[1],
                    b[0],
                    b[1],
                    d[0],
                    d[1],
                ])
            }
            Action::Multiply(a, b, d) | Action::Divide(a, b, d) => {
                let a = calendar_operand(a)?;
                let b = calendar_operand(b)?;
                let d = multiply_result_operand(d)?;
                self.instruction(&[
                    if matches!(action, Action::Multiply(..)) {
                        0x3c00
                    } else {
                        0x3e00
                    },
                    a[0],
                    a[1],
                    b[0],
                    b[1],
                    d[0],
                    d[1],
                ])
            }
            Action::Compare(a, b, d) => {
                let a = calendar_operand(a)?;
                let b = calendar_operand(b)?;
                let dest = comparison_result_operand(*d)?;
                self.instruction(&[0x2400, a[0], a[1], b[0], b[1], dest[0], dest[1]])
            }
            Action::Pulse(inner) => {
                if matches!(
                    inner.as_ref(),
                    Action::Bits(crate::bit_tools::BitAction::IndexedWrite { .. })
                ) {
                    return Err("Bit numaralı BOUT/BSET/BRST komutlarının P biçimi yok".into());
                }
                if !matches!(
                    inner.as_ref(),
                    Action::Move(..)
                        | Action::Refresh(
                            crate::refresh_io::RefreshAction::Refresh { .. }
                                | crate::refresh_io::RefreshAction::Filter { .. }
                        )
                        | Action::FileMemory(..)
                        | Action::Matrix(..)
                        | Action::Time(..)
                        | Action::Rtc(..)
                        | Action::Timing(crate::timing_io::TimingAction::Delay { .. })
                        | Action::Search(..)
                        | Action::Transform(..)
                        | Action::Additional(..)
                        | Action::Random(..)
                        | Action::Float(..)
                        | Action::Data(..)
                        | Action::Bits(..)
                        | Action::Text(..)
                        | Action::Shift(..)
                        | Action::Wide(..)
                        | Action::IndexedScalar(..)
                        | Action::IndexedMultiply(..)
                        | Action::IndexedFloat(..)
                        | Action::IndexedRoot(..)
                        | Action::IndexedAngle(..)
                        | Action::IndexedDecimalConversion(..)
                        | Action::IndexedExpLn(..)
                        | Action::IndexedLogPow(..)
                        | Action::IndexedDesqr(..)
                        | Action::IndexedTrig(..)
                        | Action::IndexedFloatComparison(..)
                        | Action::IndexedLiteralMove(..)
                        | Action::IndexedSwapMask(..)
                        | Action::IndexedRandom(..)
                        | Action::IndexedAbsGray(..)
                        | Action::IndexedSearchToggle(..)
                        | Action::IndexedCalendarTime(..)
                        | Action::IndexedRtc(..)
                        | Action::IndexedSegment(..)
                        | Action::IndexedChecksum(..)
                        | Action::IndexedText(..)
                        | Action::IndexedAdditional(..)
                        | Action::IndexedDataProcessing(..)
                        | Action::IndexedShiftFifo(..)
                        | Action::IndexedBlockExchange(..)
                        | Action::IndexedRotation(..)
                        | Action::IndexedRealBinary(..)
                        | Action::IndexedFloatBinary(..)
                        | Action::IndexedBcd(..)
                        | Action::IndexedTruncated(..)
                        | Action::IndexedComparison(..)
                        | Action::Add(..)
                        | Action::Sub(..)
                        | Action::Unary(..)
                        | Action::Complement(..)
                        | Action::Logic(..)
                        | Action::Compare(..)
                        | Action::Multiply(..)
                        | Action::Divide(..)
                        | Action::MultiplyTruncated(..)
                        | Action::DivideQuotient(..)
                        | Action::ZoneCompare(..)
                        | Action::BlockMove(..)
                        | Action::Fill(..)
                        | Action::Exchange(..)
                        | Action::Convert(..)
                        | Action::Alarm(crate::alarm::AlarmAction::Reset)
                        | Action::StringMove(_)
                        | Action::MotionBasic(crate::motion_basic::MotionAction::Frequency { .. })
                        | Action::ModuleIo(_)
                        | Action::ParallelRun(_)
                        | Action::AnalogKnob(_)
                        | Action::Pu(crate::pu_module::PuAction {
                            kind: crate::pu_module::PuKind::Input
                                | crate::pu_module::PuKind::Limits,
                            ..
                        })
                ) {
                    return Err("Pulse form requires a supported pulse instruction".into());
                }
                let start = self.output.words.len();
                self.action(inner)?;
                // Native scan dispatcher: low byte 0x10 maintains an edge bit
                // per program counter, then calls the normal 16-bit handler.
                if !self.unencoded_ranges.contains_key(&(start as u16)) {
                    self.output.words[start] |= 0x0010;
                }
                Ok(())
            }
            Action::Unary(op, destination) => {
                let d = calendar_operand(destination.value())?;
                self.instruction(&[
                    match op {
                        WordUnary::Inc => 0x4000,
                        WordUnary::Dec => 0x4200,
                        WordUnary::Neg => 0x4a00,
                    },
                    d[0],
                    d[1],
                ])
            }
            Action::Complement(source, destination) => {
                let a = calendar_operand(source)?;
                let d = calendar_operand(destination.value())?;
                self.instruction(&[0x2c00, a[0], a[1], d[0], d[1]])
            }
            Action::Logic(op, a, b, destination) => {
                let a = calendar_operand(a)?;
                let b = calendar_operand(b)?;
                let d = calendar_operand(destination.value())?;
                self.instruction(&[
                    match op {
                        WordLogic::And => 0x4400,
                        WordLogic::Or => 0x4600,
                        WordLogic::Xor => 0x4800,
                    },
                    a[0],
                    a[1],
                    b[0],
                    b[1],
                    d[0],
                    d[1],
                ])
            }
            Action::Out(d) | Action::Set(d) => {
                writable_bit(*d)?;
                self.bit_instruction(
                    if matches!(action, Action::Out(_)) {
                        BitOp::Out
                    } else {
                        BitOp::Set
                    },
                    *d,
                )
            }
            Action::Reset(d) => {
                let operand = match d.area {
                    // RST decodes the logical index directly; ordinary numeric
                    // operands instead decode a byte offset and use tag 0x86.
                    Area::T | Area::C => {
                        if d.index > 255 {
                            return Err(format!("ES2 reset target out of range: {d}"));
                        }
                        [
                            (d.index << 8) | 0x80,
                            if d.area == Area::T { 0x0082 } else { 0x0084 },
                        ]
                    }
                    _ => {
                        writable_bit(*d)?;
                        bit_operand(internal_bit_address(*d)?)
                    }
                };
                self.instruction(&[0x0700, operand[0], operand[1]])
            }
            Action::NumericReset(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::NumericReset(self.output.action_pc.len() - 1),
                );
                let operand = calendar_operand(action.target().value())?;
                self.instruction(&[0x0700, operand[0], operand[1]])
            }
            Action::WordCounter(action) => {
                self.output.preflight.insert(
                    self.output.next_pc.len(),
                    PreflightCheck::WordCounter(self.output.action_pc.len() - 1),
                );
                let preset = match action.preset {
                    crate::word_counter::WordCounterPreset::Constant(v) => {
                        numeric_operand(&Operand::Constant(v))?
                    }
                    crate::word_counter::WordCounterPreset::Data(d) => {
                        numeric_operand(&Operand::Register(d))?
                    }
                };
                self.instruction(&[
                    0x0500,
                    (action.counter.index << 8) | 0x83,
                    preset[0],
                    preset[1],
                ])
            }
            Action::Timer(d, preset) | Action::Counter(d, preset) => {
                let timer = matches!(action, Action::Timer(..));
                if (timer && (d.area != Area::T || d.index > 127))
                    || (!timer && (d.area != Area::C || d.index >= 200))
                {
                    return Err("ES2 encoder supports TMR T0..T127 and CNT C0..C199".into());
                }
                if *preset > 32767 || (timer && *preset == 0) {
                    return Err("ES2 timer/counter preset must be 1..32767".into());
                }
                let constant = numeric_operand(&Operand::Constant(*preset as i16))?;
                self.instruction(&[
                    if timer { 0x0300 } else { 0x0500 },
                    (d.index << 8) | if timer { 0x80 } else { 0x83 },
                    constant[0],
                    constant[1],
                ])
            }
            Action::Move(source, destination) => {
                let source = calendar_operand(source)?;
                let destination = calendar_operand(destination.value())?;
                self.instruction(&[0x2800, source[0], source[1], destination[0], destination[1]])
            }
            Action::Add(a, b, destination) | Action::Sub(a, b, destination) => {
                let a = calendar_operand(a)?;
                let b = calendar_operand(b)?;
                let d = calendar_operand(destination.value())?;
                self.instruction(&[
                    if matches!(action, Action::Add(..)) {
                        0x3800
                    } else {
                        0x3A00
                    },
                    a[0],
                    a[1],
                    b[0],
                    b[1],
                    d[0],
                    d[1],
                ])
            }
        }
    }
}

#[derive(Clone, Copy)]
enum BitOp {
    Ld,
    Ldi,
    Out,
    Set,
}

fn float_operand(operand: &crate::float_ops::FloatOperand) -> Result<[u16; 4], String> {
    use crate::float_ops::FloatOperand;
    wide_operand(&match operand {
        FloatOperand::Literal { bits, .. } => WideOperand::Constant(*bits as i32),
        FloatOperand::Integer(value) => WideOperand::Constant(*value),
        FloatOperand::Register(d) => WideOperand::Register(*d),
    })
}

fn writable_bit(device: Device) -> Result<(), String> {
    if !device.writable_bit() {
        Err(format!("{device} is not a writable Y/M/S coil"))
    } else {
        Ok(())
    }
}

fn internal_bit_address(device: Device) -> Result<u16, String> {
    let max = match device.area {
        Area::S => 1023,
        Area::M => 4095,
        Area::X | Area::Y | Area::T | Area::C => 255,
        Area::D => return Err("D register cannot be encoded as a contact".into()),
    };
    if device.index > max {
        return Err(format!("ES2 device is out of range: {device}"));
    }
    Ok(match device.area {
        Area::S => device.index,
        Area::X => 0x0400 + device.index,
        Area::Y => 0x0500 + device.index,
        Area::T => 0x0600 + device.index,
        Area::C => 0x0E00 + device.index,
        Area::M if device.index >= 1536 => 0x1000 + device.index - 1536,
        Area::M => 0x0800 + device.index,
        Area::D => unreachable!(),
    })
}

fn bit_operand(address: u16) -> [u16; 2] {
    [((address & 0xFF) << 8) | 0x84, (address & 0xFF00) | 0x84]
}

pub(crate) fn comparison_result_operand(base: Device) -> Result<[u16; 2], String> {
    crate::plc::comparison_destinations(base)?;
    handler_default_bit_operand(base)
}

fn handler_default_bit_operand(base: Device) -> Result<[u16; 2], String> {
    let address = internal_bit_address(base)?;
    // Compiler bare bit descriptors carry no explicit count. Verified
    // comparison handlers supply three, BON one, DECO source n; 0x84 forces eight.
    // Keep this scoped to handlers with independently verified defaults.
    Ok([((address & 0xFF) << 8) | 0x84, (address & 0xFF00) | 0x80])
}

fn numeric_operand(operand: &Operand) -> Result<[u16; 2], String> {
    let (value, low_kind, high_kind) = match operand {
        Operand::Constant(value) => (*value as u16, 0x80, 0x80),
        Operand::Register(d) => match d.area {
            Area::D if d.index <= 9999 => (d.index * 2, 0x86, 0x86),
            Area::T if d.index <= 255 => (d.index * 2, 0x86, 0x82),
            Area::C if d.index < 200 => (d.index * 2, 0x86, 0x84),
            _ => return Err(format!("Invalid ES2 numeric operand: {d}")),
        },
    };
    Ok([
        ((value & 0xFF) << 8) | low_kind,
        (value & 0xFF00) | high_kind,
    ])
}

#[cfg(test)]
pub(crate) fn installed_transform_mode(mode: i32) -> bool {
    crate::data_transform::supports_installed_mode(mode)
}

fn data_value_operand(value: &crate::data_processing::DataValue) -> Result<Vec<u16>, String> {
    match value {
        crate::data_processing::DataValue::Word(v) => Ok(numeric_operand(v)?.to_vec()),
        crate::data_processing::DataValue::Double(v) => Ok(wide_operand(v)?.to_vec()),
    }
}

fn matrix_operand(
    value: &crate::additional_processing::AdditionalValue,
    _local_layout: bool,
) -> Result<Vec<u16>, String> {
    if let crate::additional_processing::AdditionalValue::Word(Operand::Register(d)) = value
        && d.area == Area::C
        && d.index >= 200
    {
        return Ok(wide_operand(&WideOperand::Register(*d))?[..2].to_vec());
    }
    additional_operand(value)
}
fn matrix_scalar_operand(
    value: &crate::calendar_operand::CalendarValue,
) -> Result<Vec<u16>, String> {
    if let crate::calendar_operand::CalendarValue::Additional(v) = value {
        matrix_operand(v, false)
    } else {
        calendar_operand(value)
    }
}
fn drum_operand(
    value: &crate::additional_processing::AdditionalValue,
    local_layout: bool,
) -> Result<Vec<u16>, String> {
    if local_layout
        && matches!(value,crate::additional_processing::AdditionalValue::Word(Operand::Register(d)) if d.area==Area::C && d.index>=200)
    {
        // The unproven raw16 counter bank stays a runtime limitation, but its
        // two-word placeholder must not erase every later instruction's PC.
        return Ok(vec![0x0080, 0x0080]);
    }
    additional_operand(value)
}
// A wide opcode uses four encoded words even for a logically narrow parameter.
fn calendar_operand_at_width(
    value: &crate::calendar_operand::CalendarValue,
    wide: bool,
) -> Result<Vec<u16>, String> {
    if value.wide() == wide {
        return calendar_operand(value);
    }
    calendar_operand(&crate::calendar_operand::CalendarValue::parse(
        &value.to_string(),
        wide,
    )?)
}
fn multiply_result_operand(
    value: &crate::multiply_destination::MultiplyDestination,
) -> Result<Vec<u16>, String> {
    let mut words = calendar_operand(value.value())?;
    // Word full results use the 32-bit numeric storage descriptor, but only
    // its first two encoded words. This also preserves natural C200 tags.
    words.truncate(if value.wide() { 4 } else { 2 });
    Ok(words)
}

pub(crate) fn bit_index_operand(
    value: &crate::bit_index_operand::BitIndexValue,
) -> Result<Vec<u16>, String> {
    if let Some(value) = value.calendar_value() {
        calendar_operand(value)
    } else {
        Ok(value
            .native_words()
            .expect("validated double F index descriptor"))
    }
}

pub(crate) fn mask_random_destination_operand(
    value: &crate::mask_random_operand::MaskRandomDestination,
) -> Result<Vec<u16>, String> {
    if let Some(value) = value.calendar_destination() {
        calendar_operand(value.value())
    } else {
        Ok(value
            .native_words()
            .expect("validated double F destination descriptor"))
    }
}

pub(crate) fn calendar_operand(
    value: &crate::calendar_operand::CalendarValue,
) -> Result<Vec<u16>, String> {
    match value {
        crate::calendar_operand::CalendarValue::Additional(v) => additional_operand(v),
        _ => Ok(value
            .native_words()
            .expect("validated calendar index descriptor")),
    }
}
fn additional_operand(
    value: &crate::additional_processing::AdditionalValue,
) -> Result<Vec<u16>, String> {
    use crate::additional_processing::AdditionalValue;
    match value {
        AdditionalValue::Word(v) => Ok(numeric_operand(v)?.to_vec()),
        AdditionalValue::Double(v) => Ok(wide_operand(v)?.to_vec()),
        AdditionalValue::Group { base, digits, wide } => {
            let address = internal_bit_address(*base)?;
            let mut words = vec![
                ((address & 0xff) << 8) | 0x84,
                (address & 0xff00) | 0x80 | (2 * digits),
            ];
            if *wide {
                words.extend([0x0080, 0x0080]);
            }
            Ok(words)
        }
    }
}

fn data_destination(device: Device) -> Result<[u16; 2], String> {
    if device.area != Area::D {
        return Err("Numeric instruction destination must be a D register".into());
    }
    numeric_operand(&Operand::Register(device))
}

fn wide_operand(operand: &WideOperand) -> Result<[u16; 4], String> {
    match operand {
        WideOperand::Constant(value) => Ok((*value as u32)
            .to_le_bytes()
            .map(|b| (u16::from(b) << 8) | 0x80)),
        WideOperand::Register(d) => {
            if d.area == Area::C && d.index >= 200 {
                crate::wide::double_devices(*d)?;
                let value = 0x200 + (d.index - 200) * 4;
                return Ok([
                    ((value & 0xff) << 8) | 0x86,
                    (value & 0xff00) | 0x84,
                    0x0080,
                    0x0080,
                ]);
            }
            let pair = numeric_operand(&Operand::Register(*d))?;
            Ok([pair[0], pair[1], 0x0080, 0x0080])
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn modern_dtm_encoding_requires_the_explicit_current_profile() {
        use crate::data_transform::NativeTransformProfile as Profile;
        for mode in [15, 30, 31, 32, 33, 34, 35, 36] {
            for suffix in ["", "P"] {
                let p =
                    crate::plc::compile_il(&format!("LD X0\nDTM{suffix} D0 D20 K{mode} K4\nEND"))
                        .unwrap();
                assert!(super::encode(&p).is_err());
                let encoded = super::encode_for_transform_profile(&p, Profile::CurrentEs2).unwrap();
                assert_eq!(
                    &encoded.words[1..10],
                    &[
                        if suffix.is_empty() { 0x9800 } else { 0x9810 },
                        0x0086,
                        0x0086,
                        0x2886,
                        0x0086,
                        ((mode as u16) << 8) | 0x80,
                        0x0080,
                        0x0480,
                        0x0080
                    ]
                );
                assert_eq!(encoded.next_pc, [1, 10, 0]);
                assert_eq!(
                    encoded.preflight.values().collect::<Vec<_>>(),
                    vec![&super::PreflightCheck::Transform(0)]
                );
            }
        }
        for mode in [10, 39, 52] {
            let p = crate::plc::compile_il(&format!("LD X0\nDTM D0 D20 K{mode} K1\nEND")).unwrap();
            assert!(super::encode_for_transform_profile(&p, Profile::CurrentEs2).is_err());
        }
    }
    #[test]
    fn matrix_genuine_headers_lengths_pulse_and_order() {
        for (body, header, width) in [
            ("MAND D0 D10 D20 K3", 0x8601, 9),
            ("MOR D0 D10 D20 K3", 0x8801, 9),
            ("MXOR K2M0 D10 D20 D40", 0x8a01, 9),
            ("MXNR D0 D10 K4M100 K3", 0x8c01, 9),
            ("MINV D0 D20 K3", 0x8e01, 7),
            ("MCMP D0 D10 K3 E0", 0x9001, 9),
            ("MBRD D0 K3 F1", 0x9201, 7),
            ("MBWR K1X0 K3 D40", 0x9401, 7),
            ("MBS D0 D20 K3", 0x9601, 7),
            ("MBR D0 D20 K3", 0x9801, 7),
            ("MBC D0 K3 E7", 0x9a01, 7),
        ] {
            for pulse in [false, true] {
                let (name, args) = body.split_once(' ').unwrap();
                let body = format!("{name}{} {args}", if pulse { "P" } else { "" });
                let p = crate::plc::compile(&format!("LD X0\n{body}\nOUT Y0\nEND")).unwrap();
                let e = super::encode(&p).unwrap();
                assert_eq!(e.words[1], header + if pulse { 0x10 } else { 0 }, "{body}");
                assert_eq!(e.action_pc, [1, 1 + width], "{body}");
                assert_eq!(e.preflight.get(&1), Some(&super::PreflightCheck::Matrix(0)));
            }
        }
    }
    use super::*;
    use crate::plc::compile;
    #[test]
    fn alternate_bare_bit_descriptors_retain_handler_default_width() {
        for (target, descriptor) in [
            ("Y0", [0x0084, 0x0580]),
            ("M80", [0x5084, 0x0880]),
            ("S1023", [0xff84, 0x0380]),
        ] {
            for (op, header) in [("ALT", 0x9400), ("ALTP", 0x9410)] {
                let p = compile(&format!("LD X0\n{op} {target}\nEND")).unwrap();
                let encoded = encode(&p).unwrap();
                assert_eq!(
                    &encoded.words[1..4],
                    &[header, descriptor[0], descriptor[1]]
                );
                assert_eq!(encoded.action_pc, [1]);
                assert_eq!(encoded.next_pc, [1, 4, 0]);
            }
        }
    }
    #[test]
    fn search_state_transform_headers_and_preflight_bind_source_occurrences() {
        for (body, header, width) in [
            ("SER D0 K1 D300 K3", 0x8a00, 9),
            ("SERP K2M0 K1 K2M100 K3", 0x8a10, 9),
            ("DSER D0 K1 D300 K3", 0x8b00, 17),
            ("DSERP K8M0 K1 K8M100 K3", 0x8b10, 17),
            ("ALT Y0", 0x9400, 3),
            ("ALTP M1029", 0x9410, 3),
            ("RAMP D0 D1 D2 K3", 0x9600, 9),
            ("DRAMP D0 D2 D4 K3", 0x9700, 17),
            ("DTM D0 D300 K0 K3", 0x9800, 9),
            ("DTMP D0 D300 K19 K3", 0x9810, 9),
            ("SORT D0 K3 K2 D300 K1", 0x9a00, 11),
            ("DSORT D0 K3 K2 D300 K1", 0x9b00, 21),
        ] {
            let e = encode(&compile(&format!("LD X0\n{body}\nEND")).unwrap()).unwrap();
            assert_eq!(e.words[1], header, "{body}");
            assert_eq!(e.next_pc, [1, 1 + width, 0], "{body}");
        }
        let p = compile(
            "LD X0\nSERP D0 K1 D300 D10\nDTMP D20 D400 D11 K3\nSORT D40 K1 K1 D500 D12\nEND",
        )
        .unwrap();
        let e = encode(&p).unwrap();
        assert_eq!(e.action_pc, [1, 10, 19]);
        assert_eq!(
            e.preflight.values().copied().collect::<Vec<_>>(),
            [
                PreflightCheck::Search(0),
                PreflightCheck::Transform(1),
                PreflightCheck::Sort(2)
            ]
        );
        assert_eq!(p.native_program_words, 31);
        assert!(!installed_transform_mode(0x010f));
        assert!(installed_transform_mode(0x0114));
    }
    #[test]
    fn search_index_needle_descriptors_are_literal_for_all_four_forms() {
        for (name, needle, normal) in [
            (
                "SER",
                "E0",
                vec![
                    0x8a00, 0x0086, 0x0086, 0x3886, 0x0080, 0x2886, 0x0086, 0x0380, 0x0080,
                ],
            ),
            (
                "SER",
                "F0",
                vec![
                    0x8a00, 0x0086, 0x0086, 0x3a86, 0x0080, 0x2886, 0x0086, 0x0380, 0x0080,
                ],
            ),
            (
                "SER",
                "F7",
                vec![
                    0x8a00, 0x0086, 0x0086, 0x8686, 0x0180, 0x2886, 0x0086, 0x0380, 0x0080,
                ],
            ),
            (
                "DSER",
                "E0",
                vec![
                    0x8b00, 0x0086, 0x0086, 0x0080, 0x0080, 0x3886, 0x0080, 0x0080, 0x0080, 0x2886,
                    0x0086, 0x0080, 0x0080, 0x0380, 0x0080, 0x0080, 0x0080,
                ],
            ),
            (
                "DSER",
                "E1",
                vec![
                    0x8b00, 0x0086, 0x0086, 0x0080, 0x0080, 0x6c86, 0x0180, 0x0080, 0x0080, 0x2886,
                    0x0086, 0x0080, 0x0080, 0x0380, 0x0080, 0x0080, 0x0080,
                ],
            ),
            (
                "DSER",
                "E7",
                vec![
                    0x8b00, 0x0086, 0x0086, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080, 0x2886,
                    0x0086, 0x0080, 0x0080, 0x0380, 0x0080, 0x0080, 0x0080,
                ],
            ),
        ] {
            for pulse in [false, true] {
                let suffix = if pulse { "P" } else { "" };
                let source = format!("LD X0\n{name}{suffix} D0 {needle} D20 K3\nEND");
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let mut expected = normal.clone();
                if pulse {
                    expected[0] |= 0x10;
                }
                assert_eq!(&encoded.words[1..1 + expected.len()], expected, "{source}");
                assert_eq!(encoded.action_pc, [1], "{source}");
                assert_eq!(
                    encoded.next_pc,
                    [1, 1 + expected.len() as u16, 0],
                    "{source}"
                );
                assert_eq!(
                    encoded.preflight.get(&1),
                    Some(&if name == "DSER" {
                        PreflightCheck::IndexedSearchToggle(0)
                    } else {
                        PreflightCheck::Search(0)
                    }),
                    "{source}"
                );
            }
        }
    }
    #[test]
    fn swap_mask_random_index_descriptors_are_literal_for_all_twelve_forms() {
        for (name, operands, normal) in [
            ("SWAP", "F7", vec![0x4601, 0x8686, 0x0180]),
            ("DSWAP", "E1", vec![0x4701, 0x6c86, 0x0180, 0x0080, 0x0080]),
            (
                "MVM",
                "F1 E7 F7",
                vec![0x6e01, 0x6e86, 0x0180, 0x8486, 0x0180, 0x8686, 0x0180],
            ),
            (
                "DMVM",
                "F0 F1 F7",
                vec![
                    0x6f01, 0x3a86, 0x0080, 0x0080, 0x0080, 0x6e86, 0x0180, 0x0080, 0x0080, 0x8686,
                    0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "RAND",
                "F1 E7 F7",
                vec![0x5401, 0x6e86, 0x0180, 0x8486, 0x0180, 0x8686, 0x0180],
            ),
            (
                "DRAND",
                "F1 F1 F7",
                vec![
                    0x5501, 0x6e86, 0x0180, 0x0080, 0x0080, 0x6e86, 0x0180, 0x0080, 0x0080, 0x8686,
                    0x0180, 0x0080, 0x0080,
                ],
            ),
        ] {
            for pulse in [false, true] {
                let suffix = if pulse { "P" } else { "" };
                let source = format!("LD X0\n{name}{suffix} {operands}\nEND");
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let mut expected = normal.clone();
                if pulse {
                    expected[0] |= 0x10;
                }
                assert_eq!(&encoded.words[1..1 + expected.len()], expected, "{source}");
                assert_eq!(encoded.action_pc, [1], "{source}");
                if name == "DRAND" {
                    // Disabled native DRAND lands in its operand tail. The
                    // already-audited interior-PC mapping must survive F tags.
                    assert_eq!(
                        encoded.next_pc,
                        [1, 8, 9, 10, 11, 12, 13, 14, 0],
                        "{source}"
                    );
                    assert_eq!(encoded.padded_short_circuit.get(&1), Some(&14), "{source}");
                } else {
                    assert_eq!(
                        encoded.next_pc,
                        [1, 1 + expected.len() as u16, 0],
                        "{source}"
                    );
                }
            }
        }
    }
    #[test]
    fn additional_headers_packed_bits_and_drand_interior_landing_are_explicit() {
        for (body, header, width) in [
            ("SWAP D0", 0x4601, 3),
            ("DSWAPP K8M0", 0x4711, 5),
            ("MVM D0 HFF00 D4", 0x6e01, 7),
            ("DMVMP K8M0 H12345678 D4", 0x6f11, 13),
            ("WSUM D0 K3 C200", 0x8401, 7),
            ("DWSUMP C199 K2 D20", 0x8511, 13),
            ("SCALP K500 K168 K-4 D0", 0xb211, 9),
            ("SCLP K500 D0 D20", 0xb401, 7),
            ("DSCLPP D10 D0 D20", 0xb511, 13),
            ("CMPT D0 D20 H0108 D100", 0xb801, 9),
            ("DCMPTP D0 D40 H0320 D100", 0xb911, 17),
            ("RANDP K1 K9 K4M0", 0x5411, 7),
        ] {
            let e = encode(&compile(&format!("LD X0\n{body}\nEND")).unwrap()).unwrap();
            assert_eq!(e.words[1], header, "{body}");
            assert_eq!(e.next_pc, [1, 1 + width, 0], "{body}");
        }
        let e = encode(
            &compile("LD X0\nDSWAP K8M0\nDRANDP K0 K2147483647 D20\nMOV K123 D30\nEND").unwrap(),
        )
        .unwrap();
        assert_eq!(&e.words[2..6], &[0x0084, 0x0890, 0x0080, 0x0080]);
        assert_eq!(e.action_pc, [1, 6, 19]);
        assert_eq!(e.padded_short_circuit.get(&2), Some(&19));
        assert_eq!(&e.next_pc[2..9], &[13, 14, 15, 16, 17, 18, 19]);
        assert_eq!(e.words[6], 0x5511);
        let local = compile("LD X0\nMMOVP K-1 D0\nEND").unwrap();
        let encoded = encode(&local).unwrap();
        assert_eq!(encoded.words[1], 0x7e11);
        assert_eq!(encoded.next_pc, [1, 6, 0]);
    }
    #[test]
    fn bit_tools_native_action_headers_widths_and_register_tags() {
        for (body, header, width) in [
            ("ABS T255", 0xbe00, 3),
            ("DABSP C255", 0xbf10, 5),
            ("GRY K6513 D0", 0x7201, 5),
            ("DGRYP K123456789 C200", 0x7311, 9),
            ("GBIN D0 C199", 0x7401, 5),
            ("DGBINP C200 D0", 0x7511, 9),
            ("BOUT C199 D0", 0x3402, 5),
            ("DBOUT C255 D0", 0x3502, 9),
            ("BSET T255 K15", 0x3602, 5),
            ("DBRST C200 K31", 0x3902, 9),
        ] {
            let e = encode(&compile(&format!("LD X0\n{body}\nEND")).unwrap()).unwrap();
            assert_eq!(e.words[1], header, "{body}");
            assert_eq!(e.next_pc, [1, 1 + width, 0], "{body}");
        }
    }
    #[test]
    fn float_headers_operand_tags_and_native_widths() {
        for (body, header, width) in [
            ("FLT D0 D20", 0x7200, 5),
            ("DFLTP D0 D20", 0x7310, 9),
            ("INT C200 T255", 0x2201, 5),
            ("DINTP C200 C255", 0x2311, 9),
            ("DMOVR F-0.0 C200", 0xf100, 9),
            ("DECMPP D0 K23 M10", 0xed10, 13),
            ("DEZCPP K0 K10 D0 Y0", 0xef10, 17),
            ("DRAD K180 D0", 0xf900, 9),
            ("DDEGP D0 D10", 0xfb10, 9),
            ("DEBCD D0 D10", 0xfd00, 9),
            ("DEBINP D0 D10", 0xff10, 9),
            ("DEADD HFFFFFFFF D0 D10", 0x1101, 13),
            ("DESUBP K1 D0 D10", 0x1311, 13),
            ("DEMUL D0 K2 D10", 0x1501, 13),
            ("DEDIVP D0 D2 D10", 0x1711, 13),
            ("DEXPP K1 D0", 0x1911, 9),
            ("DLN D0 D10", 0x1b01, 9),
            ("DLOGP K5 K125 D10", 0x1d11, 13),
            ("DESQR D0 D10", 0x1f01, 9),
            ("DPOWP K5 K3 D10", 0x2111, 13),
            ("DSIN D0 D10", 0x2501, 9),
            ("DCOSP K60 D10", 0x2711, 9),
            ("DTAN D0 D10", 0x2901, 9),
            ("DASINP D0 D10", 0x2b11, 9),
            ("DACOS D0 D10", 0x2d01, 9),
            ("DATANP K1 D10", 0x2f11, 9),
            ("DADDR F1.2 D0 D10", 0x7701, 13),
            ("DSUBRP D0 F2.2 D10", 0x7911, 13),
            ("DMULR F-0.0 F2 D10", 0x7b01, 13),
        ] {
            let e = encode(&compile(&format!("LD X0\n{body}\nEND")).unwrap()).unwrap();
            assert_eq!(e.words[1], header, "{body}");
            assert_eq!(e.next_pc, [1, 1 + width, 0], "{body}");
        }
        let e = encode(&compile("LD X0\nDMOVR F-0.0 D0\nEND").unwrap()).unwrap();
        assert_eq!(&e.words[2..6], &[0x0080, 0x0080, 0x0080, 0x8080]);
        for (prefix, base) in [("FLD", 0x47), ("FAND", 0x53), ("FOR", 0x5f)] {
            for (relation, offset) in [
                ("=", 0),
                (">", 2),
                ("<", 4),
                ("<>", 6),
                ("<=", 8),
                (">=", 10),
            ] {
                let p = if prefix == "FLD" {
                    format!("{prefix}{relation} D0 F1.2\nOUT Y0\nEND")
                } else {
                    format!("LD X0\n{prefix}{relation} D0 F1.2\nOUT Y0\nEND")
                };
                let e = encode(&compile(&p).unwrap()).unwrap();
                assert_eq!(
                    e.words[usize::from(prefix != "FLD")],
                    ((base + offset) << 8) | 2
                );
            }
        }
    }
    #[test]
    fn text_headers_widths_and_local_only_literal() {
        for (body, header, width) in [
            ("SEGD HFFFF T255", 0xa200, 5),
            ("SEGDP D0 C199", 0xa210, 5),
            ("ASCI D0 D10 K6", 0xb400, 7),
            ("ASCIIP D0 D10 K6", 0xb410, 7),
            ("HEXP D0 D10 K6", 0xb610, 7),
            ("CCD D0 D10 D20", 0xb800, 7),
            ("LRCP D0 D20 D10", 0xd010, 7),
            ("CRC D0 K3 D10", 0xd200, 7),
        ] {
            let encoded = encode(&compile(&format!("LD X0\n{body}\nEND")).unwrap()).unwrap();
            assert_eq!(encoded.words[1], header, "{body}");
            assert_eq!(encoded.next_pc, [1, 1 + width, 0], "{body}");
        }
        let literal = compile("LD X0\nASC A b C d E f G h D10\nEND").unwrap();
        assert!(
            encode(&literal)
                .unwrap_err()
                .contains("yalnız PC ilerletir")
        );
    }
    #[test]
    fn indexed_contacts_keep_dedicated_native_prefixes() {
        let e=encode(&compile("BLD D0 K0\nBANI D0 K1\nBOR D0 K15\nOUT Y0\nDBLDI C200 K31\nDBAND C200 D0\nDBORI C200 K0\nOUT Y1\nEND").unwrap()).unwrap();
        assert_eq!(
            [e.words[0], e.words[5], e.words[10]],
            [0x3a02, 0x4002, 0x4202]
        );
        assert_eq!(
            [e.words[16], e.words[25], e.words[34]],
            [0x3d02, 0x3f02, 0x4502]
        );
        assert_eq!(e.next_pc, [5, 10, 15, 16, 25, 34, 43, 44, 0]);
        assert!(e.padded_short_circuit.is_empty());
    }
    #[test]
    fn decode_encode_bare_bit_fields_use_the_compiler_default_width() {
        for (name, args, normal) in [
            (
                "DECO",
                "X377 M128 K1",
                [0x6200, 0xff84, 0x0480, 0x8084, 0x0880, 0x0180, 0x0080],
            ),
            (
                "DECO",
                "S1023 D20 K1",
                [0x6200, 0xff84, 0x0380, 0x2886, 0x0086, 0x0180, 0x0080],
            ),
            (
                "ENCO",
                "M128 D10 K2",
                [0x6400, 0x8084, 0x0880, 0x1486, 0x0086, 0x0280, 0x0080],
            ),
        ] {
            for pulse in [false, true] {
                let program = compile(&format!(
                    "LD X0\n{name}{} {args}\nEND",
                    if pulse { "P" } else { "" }
                ))
                .unwrap();
                let encoded = encode(&program).unwrap();
                let mut expected = normal;
                if pulse {
                    expected[0] |= 0x10;
                }
                assert_eq!(encoded.words[1..8], expected, "{name}");
                assert_eq!(encoded.next_pc, [1, 8, 0]);
            }
        }
    }
    #[test]
    fn data_operand_descriptors_are_literal_for_all_twenty_forms() {
        for (name, operands, normal) in [
            (
                "DECO",
                "E1 F7 K4",
                vec![0x6200, 0x6c86, 0x0180, 0x8686, 0x0180, 0x0480, 0x0080],
            ),
            (
                "ENCO",
                "F7 E1 K4",
                vec![0x6400, 0x8686, 0x0180, 0x6c86, 0x0180, 0x0480, 0x0080],
            ),
            (
                "SUM",
                "K3M128 F1",
                vec![0x6600, 0x8084, 0x0886, 0x6e86, 0x0180],
            ),
            (
                "DSUM",
                "K8S64 E7",
                vec![
                    0x6700, 0x4084, 0x0090, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "BON",
                "K1X7 Y200 F0",
                vec![0x6800, 0x0784, 0x0482, 0x8084, 0x0580, 0x3a86, 0x0080],
            ),
            (
                "DBON",
                "K8M128 S80 F1",
                vec![
                    0x6900, 0x8084, 0x0890, 0x0080, 0x0080, 0x5084, 0x0080, 0x0080, 0x0080, 0x6e86,
                    0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "MEAN",
                "K1Y7 K3M512 E7",
                vec![0x6a00, 0x0784, 0x0582, 0x0084, 0x0a86, 0x8486, 0x0180],
            ),
            (
                "DMEAN",
                "K4M128 K8S64 F7",
                vec![
                    0x6b00, 0x8084, 0x0888, 0x0080, 0x0080, 0x4084, 0x0090, 0x0080, 0x0080, 0x8686,
                    0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "SQR",
                "H7FFF D0",
                vec![0x7000, 0xff80, 0x7f80, 0x0086, 0x0086],
            ),
            (
                "DSQR",
                "H7FFFFFFF D10",
                vec![
                    0x7100, 0xff80, 0xff80, 0xff80, 0x7f80, 0x1486, 0x0086, 0x0080, 0x0080,
                ],
            ),
        ] {
            for pulse in [false, true] {
                let suffix = if pulse { "P" } else { "" };
                let source = format!("LD X0\n{name}{suffix} {operands}\nEND");
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let mut expected = normal.clone();
                if pulse {
                    expected[0] |= 0x10;
                }
                assert_eq!(&encoded.words[1..1 + expected.len()], expected, "{source}");
                assert_eq!(
                    encoded.next_pc,
                    [1, 1 + expected.len() as u16, 0],
                    "{source}"
                );
                assert_eq!(encoded.action_pc, [1], "{source}");
            }
        }
    }
    #[test]
    fn data_processing_native_headers_and_widths() {
        for (body, header, width) in [
            ("DECO X0 M0 K3", 0x6200, 7),
            ("ENCOP M0 T255 K3", 0x6410, 7),
            ("SUM HFFFF C199", 0x6600, 5),
            ("DSUMP HFFFFFFFF C200", 0x6710, 9),
            ("BON D0 Y0 D2", 0x6800, 7),
            ("DBONP C200 M0 D0", 0x6910, 13),
            ("MEAN D0 T0 D4", 0x6a00, 7),
            ("DMEANP C199 C255 D4", 0x6b10, 13),
            ("SQR K32767 D9999", 0x7000, 5),
            ("DSQRP K2147483647 D9998", 0x7110, 9),
        ] {
            let e = encode(&compile(&format!("LD X7\n{body}\nEND")).unwrap()).unwrap();
            assert_eq!(e.words[1], header, "{body}");
            assert_eq!(e.next_pc, [1, 1 + width, 0], "{body}");
        }
        let e = encode(&compile("LD X7\nDECO X0 M0 K3\nEND").unwrap()).unwrap();
        assert_eq!(
            e.words[1..8],
            [0x6200, 0x0084, 0x0480, 0x0084, 0x0880, 0x0380, 0x0080]
        );
    }
    #[test]
    fn double_indexed_bit_commands_encode_wide_f_only_in_the_index_role() {
        // The compiler's selector-2 F1 descriptor is four words. Its low
        // address differs from E1 and from the ordinary D1183 descriptor.
        for (name, header, action, gated) in [
            ("DBOUT", 0x3502, true, true),
            ("DBSET", 0x3702, true, true),
            ("DBRST", 0x3902, true, true),
            ("DBLD", 0x3b02, false, false),
            ("DBLDI", 0x3d02, false, false),
            ("DBAND", 0x3f02, false, true),
            ("DBANI", 0x4102, false, true),
            ("DBOR", 0x4302, false, true),
            ("DBORI", 0x4502, false, true),
        ] {
            let gate = if gated { "LD X0\n" } else { "" };
            let out = if action { "" } else { "OUT Y0\n" };
            let source = format!("{gate}{name} K8M0 F1\n{out}END");
            let program = compile(&source).unwrap();
            let encoded = encode(&program).unwrap();
            let expected = [
                header, 0x0084, 0x0890, 0x0080, 0x0080, 0x6e86, 0x0180, 0x0080, 0x0080,
            ];
            let start = usize::from(gated);
            assert_eq!(encoded.words[start..start + 9], expected, "{name}");
            assert_eq!(encoded.next_pc[start], (start + 9) as u16, "{name}");
            assert_eq!(
                encoded.action_pc,
                [if action {
                    start as u16
                } else {
                    (start + 9) as u16
                }],
                "{name}"
            );
            assert!(
                program.devices.contains(&"D1183".parse().unwrap()),
                "{name}"
            );
            assert!(
                program.devices.contains(&"D1184".parse().unwrap()),
                "{name}"
            );
        }
    }

    #[test]
    fn bit_tool_operands_use_literal_descriptors_for_all_eighteen_actions() {
        let mut checked = 0;
        for (name, operands, pulse_allowed, normal) in [
            ("ABS", "K4M0", true, vec![0xbe00, 0x0084, 0x0888]),
            (
                "DABS",
                "E7",
                true,
                vec![0xbf00, 0x8486, 0x0180, 0x0080, 0x0080],
            ),
            (
                "GRY",
                "K1X7 F0",
                true,
                vec![0x7201, 0x0784, 0x0482, 0x3a86, 0x0080],
            ),
            (
                "DGRY",
                "E7 K8S992",
                true,
                vec![
                    0x7301, 0x8486, 0x0180, 0x0080, 0x0080, 0xe084, 0x0390, 0x0080, 0x0080,
                ],
            ),
            (
                "GBIN",
                "F7 K1Y7",
                true,
                vec![0x7401, 0x8686, 0x0180, 0x0784, 0x0582],
            ),
            (
                "DGBIN",
                "K4M0 E0",
                true,
                vec![
                    0x7501, 0x0084, 0x0888, 0x0080, 0x0080, 0x3886, 0x0080, 0x0080, 0x0080,
                ],
            ),
            (
                "BOUT",
                "K1M32 F7",
                false,
                vec![0x3402, 0x2084, 0x0882, 0x8686, 0x0180],
            ),
            (
                "DBOUT",
                "K4Y7 E7",
                false,
                vec![
                    0x3502, 0x0784, 0x0588, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "BSET",
                "K4S0 K1X7",
                false,
                vec![0x3602, 0x0084, 0x0088, 0x0784, 0x0482],
            ),
            (
                "DBSET",
                "K8M1536 K4X7",
                false,
                vec![
                    0x3702, 0x0084, 0x1090, 0x0080, 0x0080, 0x0784, 0x0488, 0x0080, 0x0080,
                ],
            ),
            (
                "BRST",
                "K4Y7 E0",
                false,
                vec![0x3802, 0x0784, 0x0588, 0x3886, 0x0080],
            ),
            (
                "DBRST",
                "K8S992 K1M0",
                false,
                vec![
                    0x3902, 0xe084, 0x0390, 0x0080, 0x0080, 0x0084, 0x0882, 0x0080, 0x0080,
                ],
            ),
        ] {
            for pulse in [false, true] {
                if pulse && !pulse_allowed {
                    continue;
                }
                let suffix = if pulse { "P" } else { "" };
                let source = format!("LD X0\n{name}{suffix} {operands}\nEND");
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let mut expected = normal.clone();
                if pulse {
                    expected[0] |= 0x10;
                }
                assert_eq!(&encoded.words[1..1 + expected.len()], expected, "{source}");
                assert_eq!(
                    encoded.next_pc,
                    [1, 1 + expected.len() as u16, 0],
                    "{source}"
                );
                assert_eq!(encoded.action_pc, [1], "{source}");
                checked += 1;
            }
        }
        assert_eq!(checked, 18);
    }

    #[test]
    fn indexed_contacts_use_literal_group_and_index_descriptors_for_all_twelve_forms() {
        for (name, operands, gated, expected) in [
            (
                "BLD",
                "K4M0 F7",
                false,
                vec![0x3a02, 0x0084, 0x0888, 0x8686, 0x0180],
            ),
            (
                "BLDI",
                "K4M0 F7",
                false,
                vec![0x3c02, 0x0084, 0x0888, 0x8686, 0x0180],
            ),
            (
                "BAND",
                "K1Y7 K1X7",
                true,
                vec![0x3e02, 0x0784, 0x0582, 0x0784, 0x0482],
            ),
            (
                "BANI",
                "K1Y7 K1X7",
                true,
                vec![0x4002, 0x0784, 0x0582, 0x0784, 0x0482],
            ),
            (
                "BOR",
                "K4S0 E0",
                true,
                vec![0x4202, 0x0084, 0x0088, 0x3886, 0x0080],
            ),
            (
                "BORI",
                "K4S0 E0",
                true,
                vec![0x4402, 0x0084, 0x0088, 0x3886, 0x0080],
            ),
            (
                "DBLD",
                "K8M1536 E7",
                false,
                vec![
                    0x3b02, 0x0084, 0x1090, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "DBLDI",
                "K8M1536 E7",
                false,
                vec![
                    0x3d02, 0x0084, 0x1090, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "DBAND",
                "K4Y7 K4X7",
                true,
                vec![
                    0x3f02, 0x0784, 0x0588, 0x0080, 0x0080, 0x0784, 0x0488, 0x0080, 0x0080,
                ],
            ),
            (
                "DBANI",
                "K4Y7 K4X7",
                true,
                vec![
                    0x4102, 0x0784, 0x0588, 0x0080, 0x0080, 0x0784, 0x0488, 0x0080, 0x0080,
                ],
            ),
            (
                "DBOR",
                "K8S992 E0",
                true,
                vec![
                    0x4302, 0xe084, 0x0390, 0x0080, 0x0080, 0x3886, 0x0080, 0x0080, 0x0080,
                ],
            ),
            (
                "DBORI",
                "K8S992 E0",
                true,
                vec![
                    0x4502, 0xe084, 0x0390, 0x0080, 0x0080, 0x3886, 0x0080, 0x0080, 0x0080,
                ],
            ),
        ] {
            let gate = if gated { "LD X0\n" } else { "" };
            let source = format!("{gate}{name} {operands}\nOUT Y0\nEND");
            let encoded = encode(&compile(&source).unwrap()).unwrap();
            let start = usize::from(gated);
            let next = (start + expected.len()) as u16;
            assert_eq!(
                &encoded.words[start..start + expected.len()],
                expected,
                "{source}"
            );
            assert_eq!(encoded.next_pc[start], next, "{source}");
            assert_eq!(encoded.action_pc, [next], "{source}");
        }
    }

    #[test]
    fn numeric_contacts_use_dedicated_headers_and_exact_native_widths() {
        let e = encode(&compile("LD& H8000 H8000\nAND| K0 K1\nOR^ K2 K2\nOUT Y0\nDLDZ> K170 K200 K-30\nDANDZ<= K1 K2 K1\nDORZ<> K0 K0 K0\nOUT Y1\nEND").unwrap()).unwrap();
        assert_eq!(
            [e.words[0], e.words[5], e.words[10]],
            [0xbe01, 0xc601, 0xce01]
        );
        assert_eq!(
            [e.words[16], e.words[29], e.words[42]],
            [0x7102, 0x8302, 0x9302]
        );
        assert_eq!(e.next_pc, [5, 10, 15, 16, 29, 42, 55, 56, 0]);
        assert_eq!(e.words[1..5], [0x0080, 0x8080, 0x0080, 0x8080]);
        assert_eq!(e.action_pc, [15, 55]);
    }

    #[test]
    fn numeric_contacts_encode_packed_bits_and_direct_index_descriptors() {
        // Literal descriptors use the existing, independently recovered
        // packed-bit and E/F layouts, not the parser's decimal K encoding.
        for (source, expected) in [
            (
                "LD& K4X7 E0\nOUT Y0\nEND",
                vec![0xbe01, 0x0784, 0x0488, 0x3886, 0x0080],
            ),
            (
                "LD^ F7 K1M0\nOUT Y0\nEND",
                vec![0xc201, 0x8686, 0x0180, 0x0084, 0x0882],
            ),
            (
                "DLD& K8M1536 E7\nOUT Y0\nEND",
                vec![
                    0xbf01, 0x0084, 0x1090, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "LDZ= K4M0 K1Y7 K1S0\nOUT Y0\nEND",
                vec![0x7802, 0x0084, 0x0888, 0x0784, 0x0582, 0x0084, 0x0082],
            ),
            (
                "DLDZ= K8M0 K4X7 K1S0\nOUT Y0\nEND",
                vec![
                    0x7902, 0x0084, 0x0890, 0x0080, 0x0080, 0x0784, 0x0488, 0x0080, 0x0080, 0x0084,
                    0x0082, 0x0080, 0x0080,
                ],
            ),
        ] {
            let encoded = encode(&compile(source).unwrap()).unwrap();
            assert_eq!(&encoded.words[..expected.len()], expected, "{source}");
            assert_eq!(encoded.next_pc[0], expected.len() as u16, "{source}");
            assert_eq!(encoded.action_pc, [expected.len() as u16], "{source}");
        }
        let narrow = encode(&compile("LD X0\nORZ= K4M0 K1X7 K0\nOUT Y0\nEND").unwrap()).unwrap();
        assert_eq!(narrow.words[1], 0x9002);
        assert_eq!(narrow.action_pc, [14]);
        assert_eq!(narrow.padded_short_circuit.get(&1), Some(&14));
        let wide = encode(&compile("LD X0\nDORZ= K8M0 K1X7 K0\nOUT Y0\nEND").unwrap()).unwrap();
        assert_eq!(wide.words[1], 0x9102);
        assert_eq!(wide.action_pc, [14]);
        assert!(wide.padded_short_circuit.is_empty());
    }

    #[test]
    fn signed_comparisons_encode_group_and_index_sources_at_operand_width() {
        for (instruction, expected) in [
            (
                "CMP K4M0 F0 M32",
                vec![0x2400, 0x0084, 0x0888, 0x3a86, 0x0080, 0x2084, 0x0880],
            ),
            (
                "CMPP K4M0 F0 M32",
                vec![0x2410, 0x0084, 0x0888, 0x3a86, 0x0080, 0x2084, 0x0880],
            ),
            (
                "ZCP K1X7 E7 F7 M4093",
                vec![
                    0x2600, 0x0784, 0x0482, 0x8486, 0x0180, 0x8686, 0x0180, 0xfd84, 0x1980,
                ],
            ),
            (
                "ZCPP K1X7 E7 F7 M4093",
                vec![
                    0x2610, 0x0784, 0x0482, 0x8486, 0x0180, 0x8686, 0x0180, 0xfd84, 0x1980,
                ],
            ),
            (
                "DCMP K8M1536 E7 M32",
                vec![
                    0x2500, 0x0084, 0x1090, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080, 0x2084,
                    0x0880, 0x0080, 0x0080,
                ],
            ),
            (
                "DCMPP K8M1536 E7 M32",
                vec![
                    0x2510, 0x0084, 0x1090, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080, 0x2084,
                    0x0880, 0x0080, 0x0080,
                ],
            ),
            (
                "DZCP K8X7 E0 C200 S1021",
                vec![
                    0x2700, 0x0784, 0x0490, 0x0080, 0x0080, 0x3886, 0x0080, 0x0080, 0x0080, 0x0086,
                    0x0284, 0x0080, 0x0080, 0xfd84, 0x0380, 0x0080, 0x0080,
                ],
            ),
            (
                "DZCPP K8X7 E0 C200 S1021",
                vec![
                    0x2710, 0x0784, 0x0490, 0x0080, 0x0080, 0x3886, 0x0080, 0x0080, 0x0080, 0x0086,
                    0x0284, 0x0080, 0x0080, 0xfd84, 0x0380, 0x0080, 0x0080,
                ],
            ),
        ] {
            let source = format!("LD X0\n{instruction}\nEND");
            let encoded = encode(&compile(&source).unwrap()).unwrap();
            assert_eq!(
                &encoded.words[1..1 + expected.len()],
                expected,
                "{instruction}"
            );
            assert_eq!(
                encoded.next_pc,
                [1, 1 + expected.len() as u16, 0],
                "{instruction}"
            );
        }
        for (instruction, expected) in [
            ("LD= K4M0 F0", vec![0xd001, 0x0084, 0x0888, 0x3a86, 0x0080]),
            (
                "AND<= K1X7 E7",
                vec![0xea01, 0x0784, 0x0482, 0x8486, 0x0180],
            ),
            ("OR<> F7 K4S0", vec![0xf801, 0x8686, 0x0180, 0x0084, 0x0088]),
            (
                "DLD>= K8M1536 E7",
                vec![
                    0xdd01, 0x0084, 0x1090, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "DAND<= E0 K8X7",
                vec![
                    0xeb01, 0x3886, 0x0080, 0x0080, 0x0080, 0x0784, 0x0490, 0x0080, 0x0080,
                ],
            ),
            (
                "DOR< K1Y7 C200",
                vec![
                    0xf501, 0x0784, 0x0582, 0x0080, 0x0080, 0x0086, 0x0284, 0x0080, 0x0080,
                ],
            ),
        ] {
            let leading = if instruction.starts_with("LD") || instruction.starts_with("DLD") {
                ""
            } else {
                "LD X0\n"
            };
            let source = format!("{leading}{instruction}\nOUT Y0\nEND");
            let start = usize::from(!leading.is_empty());
            let encoded = encode(&compile(&source).unwrap()).unwrap();
            assert_eq!(
                &encoded.words[start..start + expected.len()],
                expected,
                "{instruction}"
            );
            assert_eq!(
                encoded.action_pc,
                [(start + expected.len()) as u16],
                "{instruction}"
            );
        }
    }

    #[test]
    fn arithmetic_sources_and_writable_operands_use_exact_group_index_descriptors() {
        for (name, operands, normal) in [
            (
                "ADD",
                "K1X7 E0 K1M0",
                vec![0x3800, 0x0784, 0x0482, 0x3886, 0x0080, 0x0084, 0x0882],
            ),
            (
                "SUB",
                "F7 K4M0 E7",
                vec![0x3a00, 0x8686, 0x0180, 0x0084, 0x0888, 0x8486, 0x0180],
            ),
            ("INC", "K1Y7", vec![0x4000, 0x0784, 0x0582]),
            ("DEC", "F0", vec![0x4200, 0x3a86, 0x0080]),
            (
                "DADD",
                "K4M0 E7 K8M1536",
                vec![
                    0x3900, 0x0084, 0x0888, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080, 0x0084,
                    0x1090, 0x0080, 0x0080,
                ],
            ),
            (
                "DSUB",
                "E0 K8X7 E7",
                vec![
                    0x3b00, 0x3886, 0x0080, 0x0080, 0x0080, 0x0784, 0x0490, 0x0080, 0x0080, 0x8486,
                    0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "DINC",
                "K8S992",
                vec![0x4100, 0xe084, 0x0390, 0x0080, 0x0080],
            ),
            ("DDEC", "C255", vec![0x4300, 0xdc86, 0x0284, 0x0080, 0x0080]),
        ] {
            for pulse in [false, true] {
                let suffix = if pulse { "P" } else { "" };
                let source = format!("LD X0\n{name}{suffix} {operands}\nEND");
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let mut expected = normal.clone();
                if pulse {
                    expected[0] |= 0x10;
                }
                assert_eq!(&encoded.words[1..1 + expected.len()], expected, "{source}");
                assert_eq!(
                    encoded.next_pc,
                    [1, 1 + expected.len() as u16, 0],
                    "{source}"
                );
                assert_eq!(encoded.action_pc, [1], "{source}");
            }
        }
        for (target, descriptor) in [("T255", [0xfe86, 0x0182]), ("C199", [0x8e86, 0x0184])] {
            let source = format!("LD X0\nADD K1 K2 {target}\nEND");
            let encoded = encode(&compile(&source).unwrap()).unwrap();
            assert_eq!(&encoded.words[6..8], descriptor, "{source}");
        }
    }

    #[test]
    fn exchange_uses_literal_writable_group_and_index_descriptors() {
        for (name, operands, normal) in [
            (
                "XCH",
                "F7 K1Y7",
                vec![0x3200, 0x8686, 0x0180, 0x0784, 0x0582],
            ),
            (
                "XCH",
                "E1 D1182",
                vec![0x3200, 0x6c86, 0x0180, 0x3c86, 0x0986],
            ),
            (
                "DXCH",
                "K4M0 E7",
                vec![
                    0x3300, 0x0084, 0x0888, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "DXCH",
                "E1 D1182",
                vec![
                    0x3300, 0x6c86, 0x0180, 0x0080, 0x0080, 0x3c86, 0x0986, 0x0080, 0x0080,
                ],
            ),
        ] {
            for pulse in [false, true] {
                let suffix = if pulse { "P" } else { "" };
                let source = format!("LD X0\n{name}{suffix} {operands}\nEND");
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let mut expected = normal.clone();
                if pulse {
                    expected[0] |= 0x10;
                }
                assert_eq!(&encoded.words[1..1 + expected.len()], expected, "{source}");
                assert_eq!(
                    encoded.next_pc,
                    [1, 1 + expected.len() as u16, 0],
                    "{source}"
                );
                assert_eq!(encoded.action_pc, [1], "{source}");
            }
        }
    }
    #[test]
    fn bcd_bin_use_literal_group_index_descriptors_for_all_eight_forms() {
        for (name, operands, normal) in [
            (
                "BCD",
                "K4M0 F0",
                vec![0x3400, 0x0084, 0x0888, 0x3a86, 0x0080],
            ),
            (
                "BIN",
                "F7 K1Y7",
                vec![0x3600, 0x8686, 0x0180, 0x0784, 0x0582],
            ),
            (
                "DBCD",
                "K4M0 E7",
                vec![
                    0x3500, 0x0084, 0x0888, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "DBIN",
                "E7 K8S992",
                vec![
                    0x3700, 0x8486, 0x0180, 0x0080, 0x0080, 0xe084, 0x0390, 0x0080, 0x0080,
                ],
            ),
        ] {
            for pulse in [false, true] {
                let suffix = if pulse { "P" } else { "" };
                let source = format!("LD X0\n{name}{suffix} {operands}\nEND");
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let mut expected = normal.clone();
                if pulse {
                    expected[0] |= 0x10;
                }
                assert_eq!(&encoded.words[1..1 + expected.len()], expected, "{source}");
                assert_eq!(
                    encoded.next_pc,
                    [1, 1 + expected.len() as u16, 0],
                    "{source}"
                );
                assert_eq!(encoded.action_pc, [1], "{source}");
            }
        }
    }
    #[test]
    fn move_logic_negation_use_literal_group_index_descriptors_for_all_24_forms() {
        for (name, operands, normal) in [
            (
                "MOV",
                "K1X7 F0",
                vec![0x2800, 0x0784, 0x0482, 0x3a86, 0x0080],
            ),
            (
                "DMOV",
                "K4M0 E7",
                vec![
                    0x2900, 0x0084, 0x0888, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "CML",
                "F7 K1Y7",
                vec![0x2c00, 0x8686, 0x0180, 0x0784, 0x0582],
            ),
            (
                "DCML",
                "K1X7 K8M1536",
                vec![
                    0x2d00, 0x0784, 0x0482, 0x0080, 0x0080, 0x0084, 0x1090, 0x0080, 0x0080,
                ],
            ),
            (
                "WAND",
                "K4M0 E0 K1M32",
                vec![0x4400, 0x0084, 0x0888, 0x3886, 0x0080, 0x2084, 0x0882],
            ),
            (
                "DAND",
                "E7 K8M0 K4M32",
                vec![
                    0x4500, 0x8486, 0x0180, 0x0080, 0x0080, 0x0084, 0x0890, 0x0080, 0x0080, 0x2084,
                    0x0888, 0x0080, 0x0080,
                ],
            ),
            (
                "WOR",
                "K1X7 F7 E7",
                vec![0x4600, 0x0784, 0x0482, 0x8686, 0x0180, 0x8486, 0x0180],
            ),
            (
                "DOR",
                "K4M0 E0 E7",
                vec![
                    0x4700, 0x0084, 0x0888, 0x0080, 0x0080, 0x3886, 0x0080, 0x0080, 0x0080, 0x8486,
                    0x0180, 0x0080, 0x0080,
                ],
            ),
            (
                "WXOR",
                "E0 K4S0 T255",
                vec![0x4800, 0x3886, 0x0080, 0x0084, 0x0088, 0xfe86, 0x0182],
            ),
            (
                "DXOR",
                "E7 K8M1536 C200",
                vec![
                    0x4900, 0x8486, 0x0180, 0x0080, 0x0080, 0x0084, 0x1090, 0x0080, 0x0080, 0x0086,
                    0x0284, 0x0080, 0x0080,
                ],
            ),
            ("NEG", "F7", vec![0x4a00, 0x8686, 0x0180]),
            (
                "DNEG",
                "K8S992",
                vec![0x4b00, 0xe084, 0x0390, 0x0080, 0x0080],
            ),
        ] {
            for pulse in [false, true] {
                let suffix = if pulse { "P" } else { "" };
                let source = format!("LD X0\n{name}{suffix} {operands}\nEND");
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let mut expected = normal.clone();
                if pulse {
                    expected[0] |= 0x10;
                }
                assert_eq!(&encoded.words[1..1 + expected.len()], expected, "{source}");
                assert_eq!(
                    encoded.next_pc,
                    [1, 1 + expected.len() as u16, 0],
                    "{source}"
                );
                assert_eq!(encoded.action_pc, [1], "{source}");
            }
        }
    }
    #[test]
    fn full_arithmetic_uses_literal_result_descriptors_for_all_eight_forms() {
        for (name, operands, normal) in [
            (
                "MUL",
                "K1X7 F7 K1M128",
                vec![0x3c00, 0x0784, 0x0482, 0x8686, 0x0180, 0x8084, 0x0882],
            ),
            (
                "DIV",
                "K4M0 E1 E7",
                vec![0x3e00, 0x0084, 0x0888, 0x6c86, 0x0180, 0x8486, 0x0180],
            ),
            (
                "DMUL",
                "K8S0 E7 K4M32",
                vec![
                    0x3d00, 0x0084, 0x0090, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080, 0x2084,
                    0x0888, 0x0080, 0x0080,
                ],
            ),
            (
                "DDIV",
                "K4M0 E0 C199",
                vec![
                    0x3f00, 0x0084, 0x0888, 0x0080, 0x0080, 0x3886, 0x0080, 0x0080, 0x0080, 0x8e86,
                    0x0184, 0x0080, 0x0080,
                ],
            ),
            (
                "MUL",
                "K1 K2 C200",
                vec![0x3c00, 0x0180, 0x0080, 0x0280, 0x0080, 0x0086, 0x0284],
            ),
            (
                "DIV",
                "K1 K2 C255",
                vec![0x3e00, 0x0180, 0x0080, 0x0280, 0x0080, 0xdc86, 0x0284],
            ),
        ] {
            for pulse in [false, true] {
                let suffix = if pulse { "P" } else { "" };
                let source = format!("LD X0\n{name}{suffix} {operands}\nEND");
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let mut expected = normal.clone();
                if pulse {
                    expected[0] |= 0x10;
                }
                assert_eq!(&encoded.words[1..1 + expected.len()], expected, "{source}");
                assert_eq!(
                    encoded.next_pc,
                    [1, 1 + expected.len() as u16, 0],
                    "{source}"
                );
                assert_eq!(encoded.action_pc, [1], "{source}");
            }
        }
    }
    #[test]
    fn truncated_arithmetic_uses_literal_group_and_e_descriptors_for_all_eight_forms() {
        for (name, operands, normal) in [
            (
                "MUL16",
                "K4M0 E1 K1Y7",
                vec![0xf400, 0x0084, 0x0888, 0x6c86, 0x0180, 0x0784, 0x0582],
            ),
            (
                "DIV16",
                "K1X7 E0 E7",
                vec![0xf600, 0x0784, 0x0482, 0x3886, 0x0080, 0x8486, 0x0180],
            ),
            (
                "MUL32",
                "K8S0 E7 K4M32",
                vec![
                    0xf500, 0x0084, 0x0090, 0x0080, 0x0080, 0x8486, 0x0180, 0x0080, 0x0080, 0x2084,
                    0x0888, 0x0080, 0x0080,
                ],
            ),
            (
                "DIV32",
                "K4M0 E0 E7",
                vec![
                    0xf700, 0x0084, 0x0888, 0x0080, 0x0080, 0x3886, 0x0080, 0x0080, 0x0080, 0x8486,
                    0x0180, 0x0080, 0x0080,
                ],
            ),
        ] {
            for pulse in [false, true] {
                let suffix = if pulse { "P" } else { "" };
                let source = format!("LD X0\n{name}{suffix} {operands}\nEND");
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let mut expected = normal.clone();
                if pulse {
                    expected[0] |= 0x10;
                }
                assert_eq!(&encoded.words[1..1 + expected.len()], expected, "{source}");
                assert_eq!(
                    encoded.next_pc,
                    [1, 1 + expected.len() as u16, 0],
                    "{source}"
                );
                assert_eq!(encoded.action_pc, [1], "{source}");
            }
        }
    }

    #[test]
    fn truncated_native_headers_widths_and_numeric_timer_destination() {
        let p = compile("LD X0\nMUL16 K-17 K3 T255\nDIV16P K-32768 K-1 C199\nMUL32 K2147483647 K2 D0\nDIV32P C200 K-3 C255\nEND").unwrap();
        let e = encode(&p).unwrap();
        assert_eq!(e.action_pc, [1, 8, 15, 28]);
        assert_eq!(
            e.words[1..8],
            [0xf400, 0xef80, 0xff80, 0x0380, 0x0080, 0xfe86, 0x0182]
        );
        assert_eq!(e.words[8], 0xf610);
        assert_eq!(e.words[15], 0xf500);
        assert_eq!(e.words[28], 0xf710);
        assert_eq!(e.words.len(), 42);
        assert_eq!(e.words[41], 0x0f00);
    }
    #[test]
    fn native_pulse_coils_invert_and_action_pc_fixture() {
        let e = encode(&compile("LD X0\nINV\nPLS Y0\nPLF M0\nEND").unwrap()).unwrap();
        assert_eq!(
            e.words,
            [
                0x0024, 0xfdf0, 0x0701, 0x0084, 0x0584, 0x0801, 0x0084, 0x0884, 0x0f00
            ]
        );
        assert_eq!(e.next_pc, [1, 2, 5, 8, 0]);
        assert_eq!(e.action_pc, [2, 5]);
        let e = encode(&compile("LD X0\nZRST C200 C200\nEND").unwrap()).unwrap();
        assert_eq!(
            e.words,
            [0x0024, 0x6000, 0xc880, 0x0084, 0xc880, 0x0084, 0x0f00]
        );
    }
    #[test]
    fn shift_operand_descriptors_are_literal_for_all_twenty_six_forms() {
        for (name, operands, normal) in [
            (
                "ROR",
                "K4Y7 K1",
                vec![0x4c00, 0x0784, 0x0588, 0x0180, 0x0080],
            ),
            (
                "ROL",
                "K4M32 K16",
                vec![0x4e00, 0x2084, 0x0888, 0x1080, 0x0080],
            ),
            ("RCR", "F1 K3", vec![0x5000, 0x6e86, 0x0180, 0x0380, 0x0080]),
            ("RCL", "E7 K4", vec![0x5200, 0x8486, 0x0180, 0x0480, 0x0080]),
            (
                "DROR",
                "K8S0 K1",
                vec![
                    0x4d00, 0x0084, 0x0090, 0x0080, 0x0080, 0x0180, 0x0080, 0x0080, 0x0080,
                ],
            ),
            (
                "DROL",
                "K8Y7 K32",
                vec![
                    0x4f00, 0x0784, 0x0590, 0x0080, 0x0080, 0x2080, 0x0080, 0x0080, 0x0080,
                ],
            ),
            (
                "DRCR",
                "K8M32 K3",
                vec![
                    0x5100, 0x2084, 0x0890, 0x0080, 0x0080, 0x0380, 0x0080, 0x0080, 0x0080,
                ],
            ),
            (
                "DRCL",
                "E1 K4",
                vec![
                    0x5300, 0x6c86, 0x0180, 0x0080, 0x0080, 0x0480, 0x0080, 0x0080, 0x0080,
                ],
            ),
            (
                "WSFR",
                "K1X7 K1Y7 K4 K2",
                vec![
                    0x5800, 0x0784, 0x0482, 0x0784, 0x0582, 0x0480, 0x0080, 0x0280, 0x0080,
                ],
            ),
            (
                "WSFL",
                "K3M0 K3S64 K3 K2",
                vec![
                    0x5a00, 0x0084, 0x0886, 0x4084, 0x0086, 0x0380, 0x0080, 0x0280, 0x0080,
                ],
            ),
            (
                "SFWR",
                "F7 K2M128 K3",
                vec![0x5c00, 0x8686, 0x0180, 0x8084, 0x0884, 0x0380, 0x0080],
            ),
            (
                "SFRD",
                "K1S64 E0 K3",
                vec![0x5e00, 0x4084, 0x0082, 0x3886, 0x0080, 0x0380, 0x0080],
            ),
            (
                "SMOV",
                "K1X7 K4 K2 K3Y7 K3",
                vec![
                    0x2a00, 0x0784, 0x0482, 0x0480, 0x0080, 0x0280, 0x0080, 0x0784, 0x0586, 0x0380,
                    0x0080,
                ],
            ),
        ] {
            for pulse in [false, true] {
                let suffix = if pulse { "P" } else { "" };
                let source = format!("LD X0\n{name}{suffix} {operands}\nEND");
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let mut expected = normal.clone();
                if pulse {
                    expected[0] |= 0x10;
                }
                assert_eq!(&encoded.words[1..1 + expected.len()], expected, "{source}");
                assert_eq!(
                    encoded.next_pc,
                    [1, 1 + expected.len() as u16, 0],
                    "{source}"
                );
                assert_eq!(encoded.action_pc, [1], "{source}");
            }
        }
    }
    #[test]
    fn shift_step_widths_and_p_forms_follow_the_manual() {
        for (name, args, width) in [
            ("ROR", "D0 K7", 5),
            ("ROL", "D0 K7", 5),
            ("RCR", "D0 K7", 5),
            ("RCL", "D0 K7", 5),
            ("DROR", "D0 K17", 9),
            ("DROL", "D0 K17", 9),
            ("DRCR", "D0 K17", 9),
            ("DRCL", "D0 K17", 9),
            ("SFTR", "M0 M10 K5 K2", 9),
            ("SFTL", "M0 M10 K5 K2", 9),
            ("WSFR", "D0 D10 K5 K2", 9),
            ("WSFL", "D0 D10 K5 K2", 9),
            ("SFWR", "K17 D10 K4", 7),
            ("SFRD", "D10 D0 K4", 7),
            ("ZRST", "D0 D5", 5),
            ("SMOV", "D0 K4 K2 D1 K3", 11),
        ] {
            for pulse in [false, true] {
                let e = encode(
                    &compile(&format!(
                        "LD X0\n{name}{} {args}\nEND",
                        if pulse { "P" } else { "" }
                    ))
                    .unwrap(),
                )
                .unwrap();
                assert_eq!(e.next_pc, [1, 1 + width, 0], "{name}");
                assert_eq!(e.words[1] & 0x10, if pulse { 0x10 } else { 0 });
            }
        }
    }
    #[test]
    fn double_word_constants_registers_and_native_counter_encoding_fixture() {
        let e = encode(&compile("LD X0\nDMOV H87654321 C200\nDINC C255\nEND").unwrap()).unwrap();
        assert_eq!(
            e.words,
            [
                0x0024, 0x2900, 0x2180, 0x4380, 0x6580, 0x8780, 0x0086, 0x0284, 0x0080, 0x0080,
                0x4100, 0xdc86, 0x0284, 0x0080, 0x0080, 0x0f00
            ]
        );
        assert_eq!(e.next_pc, [1, 10, 15, 0]);
    }
    #[test]
    fn every_wide_variant_matches_pdf_step_width_including_pulse() {
        for (name, args, width) in [
            ("DMOV", "K1 D0", 9),
            ("DCML", "K1 D0", 9),
            ("DXCH", "D0 D2", 9),
            ("DBCD", "D0 D2", 9),
            ("DBIN", "D0 D2", 9),
            ("DFMOV", "K1 D0 K3", 13),
            ("DADD", "K1 K2 D0", 13),
            ("DSUB", "K1 K2 D0", 13),
            ("DMUL", "K1 K2 D0", 13),
            ("DDIV", "K1 K2 D0", 13),
            ("DINC", "D0", 5),
            ("DDEC", "D0", 5),
            ("DNEG", "D0", 5),
            ("DAND", "K1 K2 D0", 13),
            ("DOR", "K1 K2 D0", 13),
            ("DXOR", "K1 K2 D0", 13),
            ("DCMP", "K1 K2 M0", 13),
            ("DZCP", "K1 K2 K3 M0", 17),
        ] {
            for pulse in [false, true] {
                let p = compile(&format!(
                    "LD X0\n{name}{} {args}\nEND",
                    if pulse { "P" } else { "" }
                ))
                .unwrap();
                let e = encode(&p).unwrap();
                assert_eq!(e.next_pc, [1, 1 + width, 0], "{name}");
                assert_eq!(e.words[1] & 0x1f, if pulse { 0x10 } else { 0 });
            }
        }
        for prefix in ["DLD", "DAND", "DOR"] {
            for relation in ["=", ">", "<", "<>", "<=", ">="] {
                let source = format!(
                    "{}{prefix}{relation} K-2147483648 K2147483647\nOUT Y0\nEND",
                    if prefix == "DLD" { "" } else { "LD X0\n" }
                );
                let e = encode(&compile(&source).unwrap()).unwrap();
                let start = usize::from(prefix != "DLD");
                assert_eq!(e.next_pc[start], (start + 9) as u16);
            }
        }
    }
    #[test]
    fn bulk_numeric_headers_and_operand_order_fixture() {
        for (name, body, opcode, width) in [
            ("BMOV", "D0 D10 K3", 0x2e00, 7),
            ("FMOV", "K-7 D10 K3", 0x3000, 7),
            ("XCH", "D0 D10", 0x3200, 5),
            ("BCD", "D0 D10", 0x3400, 5),
            ("BIN", "D0 D10", 0x3600, 5),
        ] {
            for pulse in [false, true] {
                let encoded = encode(
                    &compile(&format!(
                        "LD X0\n{name}{} {body}\nEND",
                        if pulse { "P" } else { "" }
                    ))
                    .unwrap(),
                )
                .unwrap();
                assert_eq!(encoded.words[1], opcode | if pulse { 0x10 } else { 0 });
                assert_eq!(encoded.next_pc, [1, 1 + width, 0]);
            }
        }
        let p = encode(&compile("LD X0\nBMOV D0 D10 K3\nEND").unwrap()).unwrap();
        assert_eq!(
            p.words,
            [
                0x0024, 0x2e00, 0x0086, 0x0086, 0x1486, 0x0086, 0x0380, 0x0080, 0x0f00
            ]
        );
    }
    #[test]
    fn multiply_divide_and_pulse_headers_have_seven_word_width() {
        for (name, header) in [
            ("MUL", 0x3c00),
            ("MULP", 0x3c10),
            ("DIV", 0x3e00),
            ("DIVP", 0x3e10),
        ] {
            let encoded =
                encode(&compile(&format!("LD X0\n{name} K-7 K3 D0\nEND")).unwrap()).unwrap();
            assert_eq!(
                encoded.words,
                [
                    0x0024, header, 0xf980, 0xff80, 0x0380, 0x0080, 0x0086, 0x0086, 0x0f00
                ]
            );
            assert_eq!(encoded.next_pc, [1, 8, 0]);
        }
    }
    #[test]
    fn cmp_and_cmpp_use_three_bit_base_operand_fixture() {
        let encoded = encode(&compile("LD X0\nCMP K1 K2 M0\nCMPP K1 K2 M0\nEND").unwrap()).unwrap();
        assert_eq!(
            encoded.words,
            [
                0x0024, 0x2400, 0x0180, 0x0080, 0x0280, 0x0080, 0x0084, 0x0880, 0x2410, 0x0180,
                0x0080, 0x0280, 0x0080, 0x0084, 0x0880, 0x0f00
            ]
        );
        assert_eq!(encoded.next_pc, [1, 8, 15, 0]);
    }
    #[test]
    fn comparison_result_descriptors_use_handler_default_width_in_all_forms() {
        // The shipped compiler uses a zero count for bare Y/M/S. CMP/ZCP's
        // handler supplies three bits. A low tag of 0x84 forces eight bits.
        for name in [
            "CMP", "CMPP", "ZCP", "ZCPP", "DCMP", "DCMPP", "DZCP", "DZCPP",
        ] {
            for (target, expected) in [
                ("M80", [0x5084, 0x0880]),
                ("Y200", [0x8084, 0x0580]),
                ("S80", [0x5084, 0x0080]),
            ] {
                let args = if name.contains("ZCP") {
                    "K0 K2 K1"
                } else {
                    "K1 K2"
                };
                let source = format!("LD X0\n{name} {args} {target}\nEND");
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let padding = if name.starts_with('D') { 2 } else { 0 };
                let start = encoded.words.len() - 3 - padding;
                assert_eq!(&encoded.words[start..start + 2], expected, "{source}");
            }
        }
    }
    #[test]
    fn time_float_and_bit_test_targets_use_handler_default_width() {
        // Literal bare-target descriptors recovered from the compiler. Native
        // handlers select three result bits (compare) or one (BON), not eight.
        for (name, args, target_index, trailing) in [
            ("TCMP", "K1 K2 K3 D0", 10, ""),
            ("TZCP", "D0 D4 D8", 8, ""),
            ("DECMP", "D0 D4", 10, ""),
            ("DEZCP", "D0 D4 D8", 14, ""),
            ("BON", "D0", 4, " K3"),
            ("DBON", "D0", 6, " K31"),
        ] {
            for suffix in ["", "P"] {
                for (target, expected) in [
                    ("M80", [0x5084, 0x0880]),
                    ("Y200", [0x8084, 0x0580]),
                    ("S80", [0x5084, 0x0080]),
                ] {
                    let source = format!("LD X0\n{name}{suffix} {args} {target}{trailing}\nEND");
                    let encoded = encode(&compile(&source).unwrap()).unwrap();
                    assert_eq!(
                        &encoded.words[target_index..target_index + 2],
                        expected,
                        "{source}"
                    );
                }
            }
        }
    }
    #[test]
    fn block_and_fill_group_index_descriptors_are_literal() {
        for (name, args, expected) in [
            (
                "BMOV",
                "K1X0 K1M128 K3",
                vec![0x2e00, 0x0084, 0x0482, 0x8084, 0x0882, 0x0380, 0x0080],
            ),
            (
                "FMOV",
                "F1 K3Y200 K2",
                vec![0x3000, 0x6e86, 0x0180, 0x8084, 0x0586, 0x0280, 0x0080],
            ),
            (
                "DFMOV",
                "E1 K5S128 K2",
                vec![
                    0x3100, 0x6c86, 0x0180, 0x0080, 0x0080, 0x8084, 0x008a, 0x0080, 0x0080, 0x0280,
                    0x0080, 0x0080, 0x0080,
                ],
            ),
        ] {
            for suffix in ["", "P"] {
                let p = compile(&format!("LD X0\n{name}{suffix} {args}\nEND")).unwrap();
                let encoded = encode(&p).unwrap();
                let mut expected = expected.clone();
                if !suffix.is_empty() {
                    expected[0] |= 0x10;
                }
                assert_eq!(
                    &encoded.words[1..encoded.words.len() - 1],
                    expected,
                    "{name}{suffix}"
                );
            }
        }
    }
    #[test]
    fn comparison_contact_families_have_five_word_widths() {
        for (prefix, base) in [("LD", 0xd001), ("AND", 0xe001), ("OR", 0xf001)] {
            for (offset, suffix) in ["=", ">", "<", "<>", "<=", ">="].into_iter().enumerate() {
                let source = format!(
                    "{}{prefix}{suffix} H8000 H7FFF\nOUT Y0\nEND",
                    if prefix == "LD" { "" } else { "LD X0\n" }
                );
                let encoded = encode(&compile(&source).unwrap()).unwrap();
                let start = usize::from(prefix != "LD");
                assert_eq!(
                    encoded.words[start],
                    base + [0u16, 2, 4, 8, 10, 12][offset] * 0x100,
                    "{prefix}{suffix}"
                );
                assert_eq!(encoded.next_pc[start], (start + 5) as u16);
            }
        }
    }
    #[test]
    fn original_branch_stack_words_and_pc_fixture() {
        let p = compile("LD X0\nMPS\nOUT Y0\nMRD\nOUT Y1\nMPP\nOUT Y2\nEND").unwrap();
        let encoded = encode(&p).unwrap();
        assert_eq!(
            encoded.words,
            [
                0x0024, 0xfaf0, 0x00c5, 0xfbf0, 0x01c5, 0xfcf0, 0x02c5, 0x0f00
            ]
        );
        assert_eq!(encoded.next_pc, [1, 2, 3, 4, 5, 6, 7, 0]);
    }
    #[test]
    fn contact_edges_lower_to_independent_native_samples() {
        let encoded = encode(&compile("LD X0\nANDP X1\nOUT Y0\nEND").unwrap()).unwrap();
        assert_eq!(
            encoded.words,
            [0x0024, 0x0124, 0xf6f0, 0xf8f0, 0x00c5, 0x0f00]
        );
        assert_eq!(encoded.next_pc, [1, 2, 3, 4, 5, 0]);
        let falling = encode(&compile("LDF M2000\nOUT Y0\nEND").unwrap()).unwrap();
        assert_eq!(
            falling.words,
            [0x0202, 0xd084, 0x1184, 0xf7f0, 0x00c5, 0x0f00]
        );
        assert_eq!(falling.next_pc, [3, 4, 5, 0]);
    }
    #[test]
    fn result_edges_execute_after_the_complete_expression() {
        let encoded =
            encode(&compile("LD X0\nAND X1\nNP\nOUT Y0\nLD X0\nPN\nOUT Y1\nEND").unwrap()).unwrap();
        assert_eq!(
            encoded.words,
            [
                0x0024, 0x0124, 0xf8f0, 0xf6f0, 0x00c5, 0x0024, 0xf7f0, 0x01c5, 0x0f00
            ]
        );
    }
    #[test]
    fn native_inc_and_incp_fixture_preserves_pc_width() {
        let encoded = encode(&compile("LD X0\nINC D0\nINCP D0\nEND").unwrap()).unwrap();
        assert_eq!(
            encoded.words,
            [
                0x0024, 0x4000, 0x0086, 0x0086, 0x4010, 0x0086, 0x0086, 0x0f00
            ]
        );
        assert_eq!(encoded.next_pc, [1, 4, 7, 0]);
    }
    #[test]
    fn manual_step_sizes_and_verified_headers_for_numeric_forms() {
        for (body, header, width) in [
            ("MOV K1 D0", 0x2800, 5),
            ("ADD K1 K2 D0", 0x3800, 7),
            ("SUB K1 K2 D0", 0x3a00, 7),
            ("INC D0", 0x4000, 3),
            ("DEC D0", 0x4200, 3),
            ("NEG D0", 0x4a00, 3),
            ("CML K1 D0", 0x2c00, 5),
            ("WAND K1 K2 D0", 0x4400, 7),
            ("WOR K1 K2 D0", 0x4600, 7),
            ("WXOR K1 K2 D0", 0x4800, 7),
        ] {
            for pulse in [false, true] {
                let (name, args) = body.split_once(' ').unwrap();
                let body = format!("{name}{} {args}", if pulse { "P" } else { "" });
                let encoded = encode(&compile(&format!("LD X0\n{body}\nEND")).unwrap()).unwrap();
                assert_eq!(
                    encoded.words[1],
                    header | if pulse { 0x10 } else { 0 },
                    "{body}"
                );
                assert_eq!(encoded.next_pc, [1, 1 + width, 0], "{body}");
            }
        }
    }

    #[test]
    fn verified_ld_out_end_fixture() {
        let encoded = encode(&compile("LD X0\nOUT Y0\nEND").unwrap()).unwrap();
        assert_eq!(encoded.words, [0x0024, 0x00C5, 0x0F00]);
        assert_eq!(encoded.next_pc, [1, 2, 0]);
    }
    #[test]
    fn zone_comparison_nine_word_fixture_and_pulse_pc() {
        let encoded =
            encode(&compile("LD X0\nZCP K-1 K1 D0 M0\nZCPP K0 K2 D1 Y0\nEND").unwrap()).unwrap();
        assert_eq!(
            encoded.words,
            [
                0x0024, 0x2600, 0xff80, 0xff80, 0x0180, 0x0080, 0x0086, 0x0086, 0x0084, 0x0880,
                0x2610, 0x0080, 0x0080, 0x0280, 0x0080, 0x0286, 0x0086, 0x0084, 0x0580, 0x0f00
            ]
        );
        assert_eq!(encoded.next_pc, [1, 10, 19, 0]);
    }
    #[test]
    fn timer_index_and_preset_fixture() {
        let encoded = encode(&compile("LD X0\nTMR T5 K300\nEND").unwrap()).unwrap();
        assert_eq!(
            encoded.words,
            [0x0024, 0x0300, 0x0580, 0x2C80, 0x0180, 0x0F00]
        );
        assert_eq!(encoded.next_pc, [1, 5, 0]);
    }
    #[test]
    fn blocks_have_one_pc_per_executable_instruction() {
        let encoded = encode(&compile("LD X0\nOR Y0\nANI X1\nOUT Y0\nEND").unwrap()).unwrap();
        assert_eq!(
            encoded.words,
            [0x0024, 0x0025, 0xF9F0, 0x0134, 0xF8F0, 0x00C5, 0x0F00]
        );
        assert_eq!(encoded.next_pc, [1, 2, 3, 4, 5, 6, 0]);
    }
    #[test]
    fn extended_m_contact_and_output_fixture() {
        let encoded = encode(&compile("LD M2000\nOUT M2001\nEND").unwrap()).unwrap();
        assert_eq!(
            encoded.words,
            [0x0202, 0xD084, 0x1184, 0x0C02, 0xD184, 0x1184, 0x0F00]
        );
        assert_eq!(encoded.next_pc, [3, 6, 0]);
    }
    #[test]
    fn inverted_contacts_use_verified_ld_i_opcode() {
        let basic = encode(&compile("LDI X0\nOUT Y0\nEND").unwrap()).unwrap();
        assert_eq!(basic.words, [0x0034, 0x00C5, 0x0F00]);
        let extended = encode(&compile("LDI M2000\nOUT Y0\nEND").unwrap()).unwrap();
        assert_eq!(extended.words, [0x0302, 0xD084, 0x1184, 0x00C5, 0x0F00]);
        assert_eq!(extended.next_pc, [3, 4, 0]);
    }
    #[test]
    fn reset_uses_different_bit_and_numeric_operand_tags() {
        let encoded = encode(&compile("LD X0\nRST M0\nRST T1\nRST C2\nEND").unwrap()).unwrap();
        assert_eq!(
            encoded.words,
            [
                0x0024, 0x0700, 0x0084, 0x0884, 0x0700, 0x0180, 0x0082, 0x0700, 0x0280, 0x0084,
                0x0F00
            ]
        );
        assert_eq!(encoded.next_pc, [1, 4, 7, 10, 0]);
    }
    #[test]
    fn arithmetic_and_signed_constants_are_encoded_as_raw_words() {
        let encoded =
            encode(&compile("LD X0\nMOV K-1 D0\nADD D0 K1 D1\nSUB T2 C3 D9999\nEND").unwrap())
                .unwrap();
        assert_eq!(
            encoded.words,
            [
                0x0024, 0x2800, 0xFF80, 0xFF80, 0x0086, 0x0086, 0x3800, 0x0086, 0x0086, 0x0180,
                0x0080, 0x0286, 0x0086, 0x3A00, 0x0486, 0x0082, 0x0686, 0x0084, 0x1E86, 0x4E86,
                0x0F00
            ]
        );
        assert_eq!(encoded.next_pc, [1, 6, 13, 20, 0]);
    }
    #[test]
    fn full_demo_fixture() {
        let encoded = encode(&compile(crate::project::DEMO).unwrap()).unwrap();
        assert_eq!(
            encoded.words,
            [
                0x0024, 0x0025, 0xF9F0, 0x0134, 0xF8F0, 0x00C5, 0x0025, 0x0300, 0x0080, 0x1E80,
                0x0080, 0x0026, 0x01C5, 0x0224, 0x0500, 0x0083, 0x0580, 0x0080, 0x0324, 0x0700,
                0x0080, 0x0084, 0x0025, 0x2800, 0x7D80, 0x0080, 0x0086, 0x0086, 0x0F00,
            ]
        );
        assert_eq!(
            encoded.next_pc,
            [1, 2, 3, 4, 5, 6, 7, 11, 12, 13, 14, 18, 19, 22, 23, 28, 0]
        );
    }
    #[test]
    fn empty_program_is_end_and_expansion_has_a_budget() {
        assert_eq!(encode(&compile("END").unwrap()).unwrap().words, [0x0F00]);
        // Far beyond the old 512-instruction first-version cap: still encodes.
        let source = format!("LD X0\n{}OUT Y0\nEND", "AND X1\n".repeat(300));
        assert!(encode(&compile(&source).unwrap()).unwrap().words.len() > 600);
        // One giant network is refused before recursive passes could overflow the stack.
        let source = format!("LD X0\n{}OUT Y0\nEND", "AND X1\n".repeat(8000));
        let err = compile(&source).unwrap_err();
        assert!(err.message.contains("ağ"), "{err:?}");
        // The program region (15872 WORDs) is the real budget for many small networks.
        let source = format!("{}END", "LD X0\nOUT Y0\n".repeat(8000));
        let err = match compile(&source) {
            Err(d) => d.message,
            Ok(p) => encode(&p).unwrap_err(),
        };
        assert!(
            err.contains("executable") || err.contains("15872") || err.contains("program alanı"),
            "{err}"
        );
    }

    #[test]
    fn a_large_real_world_sized_program_compiles_encodes_and_scans() {
        // About 3000 instructions (a typical 6-8k step machine program).
        let mut source = String::new();
        for i in 0..500 {
            source.push_str(&format!(
                "LD X{}\nOR M{i}\nANI X{}\nOUT M{i}\nLD M{i}\nMOV K{i} D{i}\n",
                i % 8,
                (i + 1) % 8
            ));
        }
        source.push_str("END");
        let started = std::time::Instant::now();
        let program = compile(&source).unwrap();
        let encoded = encode(&program).unwrap();
        let mut sim = crate::plc::Simulator::default();
        sim.memory.set_input("X0".parse().unwrap(), true);
        sim.scan(&program, 100);
        assert!(sim.memory.bit("M0".parse().unwrap()));
        assert_eq!(sim.memory.word("D8".parse().unwrap()), 8);
        assert!(encoded.words.len() > 3000);
        assert!(started.elapsed() < std::time::Duration::from_secs(20), "{:?}", started.elapsed());
    }
}

#[cfg(test)]
#[path = "local_layout_tests.rs"]
mod local_layout_tests;
