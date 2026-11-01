//! Resumable source VM for actual jumps, calls and loop visits.
use super::*;
use crate::execution_plan::{LogicMode, PlanOp};
use crate::program_control::{ControlOp, ControlSite};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ScanStatus {
    #[default]
    Idle,
    Running,
    Complete,
    Fault(String),
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlanStep {
    pub pc: u16,
    pub next_pc: u16,
    pub scan_boundary: bool,
}
#[derive(Debug, Clone)]
pub(super) struct ScanCursor {
    pc: u16,
    random_tail_profile: crate::random_tail::RandomTailProfile,
    calendar_tail_profile: crate::calendar_tail::CalendarTailProfile,
    rtc_service_schedule: crate::rtc_clock::RtcServiceSchedule,
    scan_ms: u64,
    scan_start_us: u64,
    ordered_timers: bool,
    interrupt_frame: Option<InterruptFrame>,
}
#[derive(Debug, Clone)]
struct InterruptFrame {
    return_pc: u16,
    region: u16,
    control: crate::program_control::ControlState,
    branch_values: BTreeMap<usize, bool>,
}
impl Simulator {
    pub(crate) fn current_plan_pc(&self) -> Option<u16> {
        self.control_cursor.as_ref().map(|c| c.pc)
    }
    pub(crate) fn begin_plan_scan(
        &mut self,
        p: &Program,
        scan_ms: u64,
        stepped: bool,
    ) -> Result<(), String> {
        if self.control_cursor.is_some() {
            return Err("Önceki tarama henüz bitmedi".into());
        }
        if self.memory.interrupts.active.is_some() {
            return Err("Kesme sırasında duran yürütme dönemi sıfırlanmadan başlatılamaz".into());
        }
        if p.execution_plan.nodes.is_empty() {
            return Err("Doğrulanmış yürütme planı yok".into());
        }
        crate::random_tail::validate_program_profile(p, self.memory.random_tail_profile)?;
        crate::calendar_tail::validate_program_profile(p, self.memory.calendar_tail_profile)?;
        if self.memory.native_numeric_scratch.is_none() {
            self.memory.native_numeric_scratch = Some(p.native_program_words as i16);
        }
        self.memory.execution_limitations.clear();
        self.memory.interrupts.configure(
            p.execution_plan.links.interrupts.clone(),
            self.memory.timing_io.now_us,
        )?;
        self.memory.watchdog.begin_scan();
        self.advance_watchdog(0, 0)?;
        crate::counter::CounterState::begin_scan(&mut self.memory);
        crate::counter::CounterState::set_running(&mut self.memory, true);
        crate::high_speed::HighSpeedState::set_running(&mut self.memory, true);
        crate::pulse_output::PulseState::set_running(&mut self.memory, true);
        crate::pulse_output::PulseState::begin_scan(&mut self.memory);
        crate::external_io::ExternalIoState::begin_scan(&mut self.memory);
        crate::motion_basic::MotionBasicState::begin_scan(&mut self.memory);
        crate::motion_extended::MotionExtendedState::begin_scan(&mut self.memory);
        crate::serial_io::SerialBus::begin_scan(&mut self.memory);
        crate::analog_knob::KnobState::service(&mut self.memory);
        self.check_clock_inputs(p)?;
        self.begin_running_scan();
        crate::sfc::SfcState::service_output_inhibit(&mut self.memory);
        let ordered_timers = p.networks.iter().flat_map(|n| &n.actions).any(|a| {
            matches!(
                if let Action::Pulse(inner) = a {
                    inner.as_ref()
                } else {
                    a
                },
                Action::Timing(TimingAction::Delay { .. })
            )
        });
        self.memory.timing_io.configure_plc_timers(ordered_timers);
        crate::timing_io::begin_scan(&mut self.memory);
        crate::pid::PidState::begin_scan(&mut self.memory, scan_ms.saturating_mul(1000));
        if stepped {
            self.control.begin_stepped_scan();
        } else {
            self.control.begin_scan();
        }
        self.power.clear();
        self.power.resize(p.networks.len(), false);
        self.action_power = p
            .networks
            .iter()
            .map(|n| vec![false; n.actions.len()])
            .collect();
        self.control_cursor = Some(ScanCursor {
            pc: 0,
            random_tail_profile: self.memory.random_tail_profile,
            calendar_tail_profile: self.memory.calendar_tail_profile,
            rtc_service_schedule: self.memory.rtc_service_schedule,
            scan_ms,
            scan_start_us: self.memory.timing_io.now_us,
            ordered_timers,
            interrupt_frame: None,
        });
        self.last_scan = ScanStatus::Running;
        Ok(())
    }
    pub(crate) fn execute_plan_step(&mut self, p: &Program) -> Result<PlanStep, String> {
        let result = self.execute_plan_step_inner(p);
        if let Err(message) = &result {
            self.control_cursor = None;
            self.last_scan = ScanStatus::Fault(message.clone());
            self.memory.execution_limitations.insert(message.clone());
            self.stop_execution_io();
        }
        result
    }
    fn execute_plan_step_inner(&mut self, p: &Program) -> Result<PlanStep, String> {
        let mut cursor = self.control_cursor.take().ok_or("Etkin tarama yok")?;
        if cursor.random_tail_profile != self.memory.random_tail_profile {
            return Err("DRAND compatibility profile changed during an active scan".into());
        }
        if cursor.calendar_tail_profile != self.memory.calendar_tail_profile {
            return Err("Calendar-time compatibility profile changed during an active scan".into());
        }
        if cursor.rtc_service_schedule != self.memory.rtc_service_schedule {
            return Err("RTC service schedule changed during an active scan".into());
        }
        self.memory
            .interrupts
            .advance_to(self.memory.timing_io.now_us)?;
        let masks = crate::interrupt::mask_flags(&self.memory);
        if let Some((id, target)) = self.memory.interrupts.dispatch(masks) {
            let at = self.memory.timing_io.now_us;
            crate::motion_extended::MotionExtendedState::feedback_interrupt(
                &mut self.memory,
                &id.to_string(),
                at,
            );
            if cursor.interrupt_frame.is_some() {
                return Err("İç içe kesme çerçevesi desteklenmiyor".into());
            }
            let region = p
                .execution_plan
                .interrupt_region(target)
                .ok_or("Kesme hedef bölgesi yok")?;
            cursor.interrupt_frame = Some(InterruptFrame {
                return_pc: cursor.pc,
                region,
                control: self.control.clone(),
                branch_values: self.memory.branch_values.clone(),
            });
            let steps = self.control.steps;
            let history = self.control.pulse_history.clone();
            self.control = crate::program_control::ControlState::new(
                self.control.profile,
                self.control.hard_step_limit,
            );
            self.control.steps = steps;
            self.control.pulse_history = history;
            cursor.pc = target;
        }
        let region = p.execution_plan.interrupt_region(cursor.pc);
        if let Some(frame) = &cursor.interrupt_frame {
            if self.control.calls.is_empty() && region != Some(frame.region) {
                return Err("Kesme gövdesinden IRET olmadan çıkıldı".into());
            }
        } else if region.is_some() {
            return Err("I gövdesine kesme olayı olmadan girilemez".into());
        }
        let time_before = self.memory.timing_io.now_us;
        let node = p
            .execution_plan
            .by_pc
            .get(&cursor.pc)
            .and_then(|i| p.execution_plan.nodes.get(*i))
            .ok_or("Yürütme PC'si komut sınırında değil")?
            .clone();
        let site = ControlSite::new(node.pc, node.fallthrough, 0);
        self.memory.native_pc = node.pc;
        for id in &node.continuations {
            self.memory
                .branch_values
                .insert(*id, self.control.logic_word & 1 != 0);
        }
        if !matches!(node.op, PlanOp::Control { .. }) {
            self.control.charge_step(site).map_err(|e| e.to_string())?;
        }
        let mut next = node.fallthrough;
        let mut boundary = false;
        match node.op {
            PlanOp::Predicate { mode, expr } => {
                let read = match mode {
                    LogicMode::Load => true,
                    LogicMode::And => self.control.logic_word & 1 != 0,
                    LogicMode::Or => self.control.logic_word & 1 == 0,
                };
                // Signed/bitwise native contacts read their operands before
                // combining the accumulator. Difference contacts instead
                // skip reads on a deciding AND/OR input.
                let read = read
                    || matches!(&expr, Expr::IndexedBit(_))
                    || matches!(&expr, Expr::IndexedComparison(p) if !p.short_circuits())
                    || matches!(&expr, Expr::IndexedFloatComparison(p) if !p.short_circuits());
                if !read && let Some(target) = node.shortcut_pc {
                    next = target;
                }
                if read {
                    if let Expr::IndexedBit(contact) = &expr {
                        contact.observed_devices(&self.memory, &mut self.observed_devices);
                    }
                    // A failed address must not become FALSE and then be
                    // inverted by a following INV into an energized output.
                    let value = if let Expr::IndexedComparison(predicate) = &expr {
                        predicate.visit(&self.memory, &mut self.observed_devices)?
                    } else if let Expr::IndexedBitNumber(predicate) = &expr {
                        predicate.visit(&self.memory, &mut self.observed_devices)?
                    } else if let Expr::IndexedFloatComparison(predicate) = &expr {
                        predicate.visit(&self.memory, &mut self.observed_devices)?
                    } else {
                        expr.sample(&mut self.memory)?
                    };
                    match mode {
                        LogicMode::Load => self.control.load(value),
                        LogicMode::And => self.control.and(value),
                        LogicMode::Or => self.control.or(value),
                    }
                }
            }
            PlanOp::Edge { rising, id } => {
                let bit = self
                    .memory
                    .sample_edge(id, rising, self.control.logic_word & 1 != 0);
                self.control.logic_word = (self.control.logic_word & !1) | u16::from(bit);
            }
            PlanOp::Invert => self.control.invert(),
            PlanOp::AndBlock => self.control.and_block(),
            PlanOp::OrBlock => self.control.or_block(),
            PlanOp::OperandTailControl { header, op } => {
                let qualified = match p.execution_plan.operand_tail_owners.get(&header) {
                    Some(crate::random_tail::OperandTailOwner::Drand) => {
                        self.memory.random_tail_profile
                            == crate::random_tail::RandomTailProfile::CurrentCommgr
                    }
                    Some(crate::random_tail::OperandTailOwner::CalendarTime) => {
                        self.memory.calendar_tail_profile
                            == crate::calendar_tail::CalendarTailProfile::CurrentCommgr
                    }
                    None => false,
                };
                if !qualified {
                    return Err(
                        "Operand-tail control needs its verified owner and current profile".into(),
                    );
                }
                crate::random_tail::execute_control(&mut self.control, op, node.pc);
            }
            PlanOp::Capture(id) => {
                self.memory
                    .branch_values
                    .insert(id, self.control.logic_word & 1 != 0);
                self.control.mps();
            }
            PlanOp::Restore(pop) => {
                if pop {
                    self.control.mpp();
                } else {
                    self.control.mrd();
                }
            }
            PlanOp::Action {
                network,
                action,
                ordinal,
            } => {
                let action_index = action;
                let action = p
                    .networks
                    .get(network)
                    .and_then(|n| n.actions.get(action))
                    .ok_or("Komut kaynağı planla eşleşmiyor")?;
                // Indexed bit writers use accumulator + output context, not
                // the ordinary drive/master gate. Even a disabled OUT writes
                // zero; indexed S targets are raw bits, not SFC transitions.
                let power = if matches!(action, Action::IndexedBit(_)) {
                    self.control.logic_word & 1 != 0 && self.control.out_enabled
                } else {
                    self.control.effective_power()
                };
                self.action_power[network][action_index] = power;
                self.power[network] = power;
                let needs_output_context = matches!(
                    action,
                    Action::Out(_)
                        | Action::NumericReset(_)
                        | Action::Timer(..)
                        | Action::SpecialTimer(crate::special_timers::TimerAction::Timer { .. })
                ) || matches!(action,Action::Reset(d) if matches!(d.area,Area::T|Area::C));
                if self.control.out_enabled || !needs_output_context {
                    self.execute_action_visit(action, power, ordinal, node.pc);
                    if let Some(target) = self.action_pc_override.take() {
                        next = target;
                    }
                }
            }
            PlanOp::Control { mut op, line } => {
                if matches!(
                    op,
                    ControlOp::InterruptLabel(_)
                        | ControlOp::InterruptReturn
                        | ControlOp::InterruptEnable
                        | ControlOp::InterruptDisable
                        | ControlOp::Watchdog { .. }
                ) {
                    self.control.charge_step(site).map_err(|e| e.to_string())?;
                    match op {
                        ControlOp::InterruptEnable => {
                            let reverse = crate::interrupt::reverse_flags(&self.memory);
                            self.memory.interrupts.on_ei(reverse);
                        }
                        ControlOp::InterruptDisable => self.memory.interrupts.on_di(),
                        ControlOp::InterruptReturn => {
                            if !self.control.calls.is_empty() || !self.control.loops.is_empty() {
                                return Err("IRET öncesi CALL/FOR çerçevesi kapanmalı".into());
                            }
                            let frame = cursor
                                .interrupt_frame
                                .take()
                                .ok_or("Etkin kesme olmadan IRET")?;
                            self.memory.interrupts.finish_handler()?;
                            let steps = self.control.steps;
                            let history = std::mem::take(&mut self.control.pulse_history);
                            self.control = frame.control;
                            self.control.steps = steps;
                            self.control.pulse_history = history;
                            self.memory.branch_values = frame.branch_values;
                            next = frame.return_pc;
                        }
                        ControlOp::Watchdog { pulse } => {
                            let power = self.control.qualify(site, pulse);
                            self.memory.watchdog.refresh(power);
                        }
                        _ => {}
                    }
                } else {
                    if let ControlOp::Step { step } = &op {
                        self.control.charge_step(site).map_err(|e| e.to_string())?;
                        let adjacent = p.execution_plan.nodes.iter().any(|prev| {
                            prev.fallthrough == node.pc
                                && prev.pc + 1 == node.pc
                                && matches!(
                                    prev.op,
                                    PlanOp::Control {
                                        op: ControlOp::Step { .. },
                                        ..
                                    }
                                )
                        });
                        crate::sfc::SfcState::enter(
                            &mut self.memory,
                            &mut self.control,
                            *step,
                            adjacent,
                        );
                        crate::sfc::SfcState::service_output_inhibit(&mut self.memory);
                        let cost = self.memory.interrupts.header_instruction_cost_us;
                        if cost != 0 {
                            crate::timing_io::advance_by(&mut self.memory, cost);
                        }
                        self.advance_watchdog(
                            self.memory.timing_io.now_us.saturating_sub(time_before),
                            node.pc,
                        )?;
                        cursor.pc = next;
                        self.control_cursor = Some(cursor);
                        return Ok(PlanStep {
                            pc: node.pc,
                            next_pc: next,
                            scan_boundary: false,
                        });
                    }
                    if matches!(op, ControlOp::StepReturn) {
                        crate::sfc::SfcState::ret(&mut self.memory, &mut self.control);
                    }
                    // Genuine natural C200 scalar reads report E1A and return zero;
                    // the FOR loop still captures max(1,0). This is not a raw block.
                    if let ControlOp::For {
                        count:
                            crate::calendar_operand::CalendarValue::Additional(
                                crate::additional_processing::AdditionalValue::Word(
                                    Operand::Register(d),
                                ),
                            ),
                    } = &op
                        && d.area == Area::C
                        && d.index >= 200
                    {
                        self.memory.operation_error(0xe1a, 1);
                        op = ControlOp::For {
                            count: crate::calendar_operand::CalendarValue::Additional(
                                crate::additional_processing::AdditionalValue::Word(
                                    Operand::Constant(0),
                                ),
                            ),
                        };
                    }
                    let transition = self
                        .control
                        .transition(
                            &op,
                            ControlSite::new(node.pc, node.fallthrough, line),
                            &self.memory,
                            &p.execution_plan.links,
                        )
                        .map_err(|e| e.to_string())?;
                    next = transition.next_pc;
                    boundary = transition.scan_boundary;
                }
            }
            PlanOp::End => {
                if cursor.interrupt_frame.is_some() {
                    return Err("Etkin kesme IRET yerine END'e ulaştı".into());
                }
                let transition = self.control.finish_scan(site).map_err(|e| e.to_string())?;
                next = transition.next_pc;
                boundary = true;
            }
            PlanOp::Inert => {}
        }
        if boundary && cursor.interrupt_frame.is_some() {
            return Err("Etkin kesme FEND ile sonlanamaz".into());
        }
        let instruction_cost = self.memory.interrupts.header_instruction_cost_us;
        if instruction_cost != 0 {
            crate::timing_io::advance_by(&mut self.memory, instruction_cost);
        }
        self.advance_watchdog(
            self.memory.timing_io.now_us.saturating_sub(time_before),
            node.pc,
        )?;
        crate::counter::CounterState::service(&mut self.memory);
        crate::system_relays::ClockState::service(&mut self.memory);
        crate::sfc::SfcState::service_output_inhibit(&mut self.memory);
        if boundary {
            self.finish_action_scan(cursor.scan_start_us, cursor.ordered_timers, cursor.scan_ms)?;
            self.last_scan = ScanStatus::Complete;
        } else {
            if !p.execution_plan.by_pc.contains_key(&next) {
                return Err(format!("Hedef PC {next} gerçek komut sınırında değil"));
            }
            cursor.pc = next;
            self.control_cursor = Some(cursor);
        }
        Ok(PlanStep {
            pc: node.pc,
            next_pc: next,
            scan_boundary: boundary,
        })
    }
    pub(super) fn run_plan_scan(&mut self, p: &Program, scan_ms: u64) {
        if let Err(message) = self.begin_plan_scan(p, scan_ms, false) {
            self.control_cursor = None;
            self.stop_execution_io();
            self.last_scan = ScanStatus::Fault(message.clone());
            self.memory.execution_limitations.insert(message);
            return;
        }
        while self.control_cursor.is_some() {
            if self.execute_plan_step(p).is_err() {
                break;
            }
        }
    }
}
