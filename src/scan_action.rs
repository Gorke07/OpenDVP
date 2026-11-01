//! One shared executor for an action visit; control loops reuse stable source IDs.
use super::*;
impl Simulator {
    fn apply_calendar_time_outcome(&mut self, outcome: crate::time_ops::CalendarTimeOutcome) {
        if outcome == crate::time_ops::CalendarTimeOutcome::ArithmeticSuccess {
            match self.memory.calendar_tail_profile {
                crate::calendar_tail::CalendarTailProfile::CurrentCommgr => {
                    self.action_pc_override = None
                }
                crate::calendar_tail::CalendarTailProfile::Documented => {}
                crate::calendar_tail::CalendarTailProfile::UnqualifiedNative => {
                    self.memory
                        .execution_limitations
                        .insert("Time arithmetic native profile is unqualified".into());
                }
            }
        }
    }

    /// Pure numerical-body qualification for paused native preflight. The
    /// actual action visit alone updates header and operand-tail histories.
    pub(crate) fn random_header_gate(
        &self,
        pulse: bool,
        ordinal: usize,
        pc: u16,
    ) -> crate::random_tail::RandomGateState {
        crate::random_tail::RandomGateState {
            pc,
            logic_word: self.control.logic_word,
            master_on: self.control.master_gate & 1 != 0,
            pulse_latched: pulse && self.action_edges.get(&ordinal).copied().unwrap_or(false),
        }
    }
    pub(super) fn stop_execution_io(&mut self) {
        crate::dmv_comm::DmvState::abort(&mut self.memory);
        crate::ethernet::EthernetState::abort(&mut self.memory);
        crate::ethernet_raw::RawEthernetState::abort(&mut self.memory);
        crate::canopen_exchange::CanExchangeState::abort(&mut self.memory);
        crate::coordinated_motion::CoordinatedState::abort(&mut self.memory);
        crate::pu_module::PuState::abort(&mut self.memory);
        crate::ftc::FtcState::abort(&mut self.memory);
        crate::ethernet_config::EthernetConfigState::abort(&mut self.memory);
        crate::canopen::CanopenNetwork::abort(&mut self.memory);
        crate::serial_io::SerialBus::abort(&mut self.memory);
        crate::motion_basic::MotionBasicState::abort(&mut self.memory);
        crate::motion_extended::MotionExtendedState::abort(&mut self.memory);
        crate::external_io::ExternalIoState::abort(&mut self.memory);
        crate::pulse_output::PulseState::abort(&mut self.memory);
        self.memory.timing_io.abort_output_engines();
        crate::refresh_io::RefreshState::abort_output_engine(&mut self.memory);
        self.memory.interrupts.on_di();
        crate::counter::CounterState::set_running(&mut self.memory, false);
        crate::high_speed::HighSpeedState::set_running(&mut self.memory, false);
        crate::system_relays::ClockState::set_running(&mut self.memory, false);
        crate::power_monitor::PowerState::service(&mut self.memory);
    }
    pub(super) fn finish_action_scan(
        &mut self,
        scan_start_us: u64,
        ordered_timers: bool,
        scan_ms: u64,
    ) -> Result<(), String> {
        // Idle-time peripheral completions belong to the next PLC body.
        crate::external_io::ExternalIoState::end_scan(&mut self.memory);
        crate::motion_basic::MotionBasicState::end_scan(&mut self.memory);
        crate::motion_extended::MotionExtendedState::end_scan(&mut self.memory);
        crate::serial_io::SerialBus::end_scan(&mut self.memory);
        crate::timing_io::finish_scan(&mut self.memory, scan_ms);
        // scan_ms is a simulation time quantum, not measured CPU execution.
        // DELAY and explicitly configured instruction costs are charged at
        // their actual visits; fixed-scan idle does not consume this watchdog.
        self.advance_watchdog(0, self.memory.native_pc)?;
        let scan_end_us = self.memory.timing_io.now_us;
        crate::special_timers::TimerState::end_scan(
            &mut self.memory,
            scan_start_us,
            scan_end_us,
            ordered_timers,
        );
        if self.memory.rtc_service_schedule == crate::rtc_clock::RtcServiceSchedule::EndOfScan {
            crate::rtc_clock::end_scan(&mut self.memory, crate::rtc_clock::RtcAdvance::Hold);
        }
        crate::system_relays::ClockState::end_scan(&mut self.memory);
        if self.observed_devices.contains(&Memory::system_device(1019)) {
            crate::power_monitor::PowerState::observe(&mut self.memory);
        }
        crate::sfc::SfcState::end_scan(&mut self.memory);
        self.finish_running_flags();
        crate::sfc::SfcState::service_output_inhibit(&mut self.memory);
        crate::refresh_io::RefreshState::end_scan(&mut self.memory);
        crate::pulse_output::PulseState::end_scan(&mut self.memory);
        for index in 0..=255 {
            let d = Device {
                area: Area::Y,
                index,
            };
            if !self.memory.timing_io.output_owners.contains_key(&d) {
                self.memory.set_physical_output(d, self.memory.bit(d));
            }
        }
        self.scans += 1;
        self.elapsed_ms = self.memory.timing_io.now_us / 1000;
        Ok(())
    }
    pub(super) fn advance_watchdog(&mut self, delta_us: u64, pc: u16) -> Result<(), String> {
        let limit = self.memory.word(Device {
            area: Area::D,
            index: 1000,
        });
        if let Err(error) = self.memory.watchdog.advance(delta_us, limit, pc) {
            self.stop_execution_io();
            if matches!(error, crate::interrupt::WatchdogError::Timeout { .. }) {
                self.memory.set_system(1008, true);
                self.memory.set_word(
                    Device {
                        area: Area::D,
                        index: 1008,
                    },
                    pc as i16,
                );
                self.memory.set_system(1072, false);
                self.memory.set_system(1000, false);
                self.memory.set_system(1001, true);
                for index in 0..=255 {
                    self.memory.set_internal(
                        Device {
                            area: Area::Y,
                            index,
                        },
                        false,
                    );
                    self.memory.set_physical_output(
                        Device {
                            area: Area::Y,
                            index,
                        },
                        false,
                    );
                }
            }
            return Err(error.to_string());
        }
        Ok(())
    }
    pub(super) fn execute_action_visit(
        &mut self,
        action: &Action,
        power: bool,
        action_index: usize,
        pc: u16,
    ) {
        self.action_pc_override = if crate::calendar_tail::is_arithmetic(action) {
            Some(pc + 7)
        } else {
            None
        };
        crate::counter::CounterState::service(&mut self.memory);
        // DRAND's active body skips all 13 WORDs, while a fresh FALSE body
        // advances only seven and dispatches the six real operand-tail WORDs.
        // A latched P site skips 13 on held TRUE and on its first FALSE visit.
        // Capture that history before the ordinary pulse gate consumes it.
        let drand_skip_tail = crate::random_tail::is_drand(action)
            && (self.memory.random_tail_profile
                == crate::random_tail::RandomTailProfile::Documented
                || power
                || (matches!(action, Action::Pulse(_))
                    && self
                        .action_edges
                        .get(&action_index)
                        .copied()
                        .unwrap_or(false)));
        let (action, power) = if let Action::Pulse(inner) = action {
            let was = self
                .action_edges
                .insert(action_index, power)
                .unwrap_or(false);
            if was
                && matches!(
                    inner.as_ref(),
                    Action::Alarm(crate::alarm::AlarmAction::Reset)
                )
            {
                // Original ANRP skips the handler entirely on held TRUE and
                // the first FALSE. A fresh FALSE still visits its monitor.
                return;
            }
            (inner.as_ref(), power && !was)
        } else {
            (action, power)
        };
        self.memory.native_pc = pc;
        if drand_skip_tail {
            self.action_pc_override = Some(pc + 13);
        }
        if let Action::Float(crate::float_ops::FloatAction::Math(math)) = action
            && math.op == crate::float_math::MathOp::DivideReal
        {
            if power && math.execute(&mut self.memory) {
                self.action_pc_override = Some(pc + 17);
            }
            return;
        }
        if let Action::IndexedAngle(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedDecimalConversion(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedExpLn(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedLogPow(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedDesqr(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedTrig(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedFloatComparison(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedLiteralMove(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedSwapMask(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedRandom(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedBitNumber(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedAbsGray(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedSearchToggle(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedAdditional(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::Additional(
            legacy @ (crate::additional_processing::AdditionalAction::WordSum { .. }
            | crate::additional_processing::AdditionalAction::Scale { .. }
            | crate::additional_processing::AdditionalAction::ScalePoints { .. }
            | crate::additional_processing::AdditionalAction::CompareTable { .. }),
        ) = action
        {
            match crate::indexed_additional::IndexedAdditionalAction::from_legacy(legacy) {
                Ok(indexed) => {
                    indexed.execute(&mut self.memory, power, &mut self.observed_devices);
                }
                Err(reason) if power => {
                    self.memory.execution_limitations.insert(reason);
                }
                Err(_) => {}
            }
            return;
        }
        if let Action::IndexedText(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::Text(
            legacy @ (crate::text_encoding::TextAction::Ascii { .. }
            | crate::text_encoding::TextAction::Hex { .. }
            | crate::text_encoding::TextAction::Check { .. }),
        ) = action
        {
            match crate::indexed_text::IndexedTextAction::from_legacy(legacy) {
                Ok(indexed) => {
                    indexed.execute(&mut self.memory, power, &mut self.observed_devices);
                }
                Err(reason) if power => {
                    self.memory.execution_limitations.insert(reason);
                }
                Err(_) => {}
            }
            return;
        }
        if let Action::IndexedChecksum(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::Text(
            legacy @ (crate::text_encoding::TextAction::Lrc { .. }
            | crate::text_encoding::TextAction::Crc { .. }),
        ) = action
        {
            match crate::indexed_checksum::IndexedChecksumAction::from_legacy(legacy) {
                Ok(indexed) => {
                    indexed.execute(&mut self.memory, power, &mut self.observed_devices);
                }
                Err(reason) if power => {
                    self.memory.execution_limitations.insert(reason);
                }
                Err(_) => {}
            }
            return;
        }
        if let Action::IndexedAsc(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedSegment(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedRtc(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::Rtc(legacy) = action {
            crate::indexed_rtc::IndexedRtcAction::from_legacy(legacy).execute(
                &mut self.memory,
                power,
                &mut self.observed_devices,
            );
            return;
        }
        if let Action::IndexedCalendarTime(indexed) = action {
            let outcome = indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            self.apply_calendar_time_outcome(outcome);
            return;
        }
        if let Action::IndexedDataProcessing(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedShiftFifo(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedBlockExchange(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedRotation(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedRealBinary(indexed) = action {
            if indexed.execute(&mut self.memory, power, &mut self.observed_devices) {
                self.action_pc_override = Some(pc + 17);
            }
            return;
        }
        if let Action::IndexedFloatBinary(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedRoot(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedFloat(indexed) = action {
            indexed.execute(&mut self.memory, power, &mut self.observed_devices);
            return;
        }
        if let Action::IndexedBit(indexed) = action {
            if !indexed.should_resolve(power) {
                return;
            }
            match indexed.resolve(&self.memory) {
                Ok(resolved) => {
                    let dest = resolved.destination();
                    self.observed_devices.insert(dest);
                    // The native generic writer has no SFC predecessor/M1040
                    // side effects, including when the target is an S bit.
                    let value = match resolved {
                        Action::Out(_) => power,
                        Action::Set(_) => true,
                        Action::Reset(_) => false,
                        _ => unreachable!("indexed bit resolver"),
                    };
                    self.memory.set_bit(dest, value);
                }
                Err(reason) => {
                    self.memory.execution_limitations.insert(reason);
                }
            }
            return;
        }
        // Resolve modifiers at the actual visit after earlier writes to E/F.
        // The ordinary executor retains arithmetic flags and write semantics.
        let indexed_result = if power {
            match action {
                Action::IndexedScalar(indexed) => {
                    Some(indexed.prepare(&self.memory, &mut self.observed_devices))
                }
                Action::IndexedComparison(indexed) => {
                    Some(indexed.prepare(&self.memory, &mut self.observed_devices))
                }
                Action::IndexedBcd(indexed) => {
                    use crate::indexed_bcd::PreparedBcd;
                    match indexed.prepare(&self.memory, &mut self.observed_devices) {
                        Ok(PreparedBcd::Ready(action)) => Some(Ok(*action)),
                        Ok(PreparedBcd::InvalidSource) => {
                            self.memory.operation_error(0xe18, 1);
                            None
                        }
                        Err(reason) => Some(Err(reason)),
                    }
                }
                Action::IndexedTruncated(indexed) => {
                    use crate::indexed_multiply::PreparedMultiply;
                    match indexed.prepare(&self.memory, &mut self.observed_devices) {
                        Ok(PreparedMultiply::Ready(action)) => Some(Ok(*action)),
                        Ok(PreparedMultiply::ZeroDivisor { offset }) => {
                            self.memory.operation_error(0xe19, offset);
                            None
                        }
                        Err(reason) => Some(Err(reason)),
                    }
                }
                Action::IndexedMultiply(indexed) => {
                    use crate::indexed_multiply::PreparedMultiply;
                    match indexed.prepare(&self.memory, &mut self.observed_devices) {
                        Ok(PreparedMultiply::Ready(action)) => Some(Ok(*action)),
                        Ok(PreparedMultiply::ZeroDivisor { offset }) => {
                            self.memory.operation_error(0xe19, offset);
                            None
                        }
                        Err(reason) => Some(Err(reason)),
                    }
                }
                _ => None,
            }
        } else {
            None
        };
        let resolved = if let Some(result) = indexed_result {
            match result {
                Ok(resolved) => {
                    resolved.source_devices(&mut self.observed_devices);
                    self.observed_devices.insert(resolved.destination());
                    Some(resolved)
                }
                Err(reason) => {
                    self.memory.execution_limitations.insert(reason);
                    None
                }
            }
        } else {
            None
        };
        let action = resolved.as_ref().unwrap_or(action);
        if let Action::EdgeOut(dest, rising) = action {
            let was = self
                .action_edges
                .insert(action_index, power)
                .unwrap_or(false);
            self.memory.set_internal(
                *dest,
                if *rising {
                    power && !was
                } else {
                    !power && was
                },
            );
            return;
        }
        let action_index = action_index + 1;
        let d = action.destination();
        match action {
            Action::ParallelRun(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::AnalogKnob(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::Drive(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::Servo(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::Dmv(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::Ethernet(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::RawEthernet(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::Canopen(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::CanExchange(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::CoordinatedMotion(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::Pu(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::ExtendedMotion(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::SerialIo(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::ModuleIo(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::MotionBasic(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::ExternalIo(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::PulseOutput(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::Refresh(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, usize::from(pc));
                if power
                    && let crate::refresh_io::RefreshAction::Refresh { start, count } = action
                    && start.area == Area::Y
                {
                    for (channel, enable) in [(0, 1672), (2, 1674)] {
                        if self.memory.system_bit(enable)
                            && (start.index..start.index.saturating_add(*count)).contains(&channel)
                        {
                            crate::motion_extended::MotionExtendedState::refresh_table(
                                &mut self.memory,
                                channel as u8,
                            );
                        }
                    }
                }
            }
            Action::HighSpeed(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::DoubleCounter(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::EthernetConfig(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::Ftc(action) => {
                action.devices(&mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::Pid(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::Station(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, action_index - 1);
            }
            Action::StringMove(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power);
            }
            Action::Alarm(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::Sfc(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, pc);
            }
            Action::SpecialTimer(action) => {
                if let Some(result) = action.execute(&mut self.memory, power, pc) {
                    self.control.and(result);
                }
            }
            Action::FileMemory(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, action_index - 1);
            }
            Action::Drum(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                let pc = self.memory.native_instruction_pc();
                action.execute(&mut self.memory, power, pc);
            }
            Action::Matrix(action) if power => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory);
            }
            Action::Hour(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                let pc = self.memory.native_instruction_pc();
                let now_us = self.memory.timing_io.now_us;
                action.execute(&mut self.memory, power, pc, now_us);
            }
            Action::Time(action) if power => {
                let outcome = action.execute(&mut self.memory);
                self.apply_calendar_time_outcome(outcome);
            }
            Action::Timing(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, action_index - 1, power);
            }
            Action::Gps(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                let now_us = self.memory.timing_io.now_us;
                action.execute(&mut self.memory, power, action_index - 1, now_us);
            }
            Action::Solar(action) if power => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory);
            }
            Action::Search(action) if power => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory);
            }
            Action::RampSort(action) => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory, power, action_index - 1);
            }
            Action::Transform(action) if power => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory);
            }
            Action::Additional(action) if power => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory);
            }
            Action::Random(action) if power => action.execute(&mut self.memory),
            Action::Float(action) if power => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory);
            }
            Action::Bits(action) => action.execute(&mut self.memory, power),
            Action::Text(action) if power => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory);
            }
            Action::Data(action) if power => {
                action.observed_devices(&self.memory, &mut self.observed_devices);
                action.execute(&mut self.memory);
            }
            Action::Shift(action) if power => {
                action.execute(&mut self.memory);
            }
            Action::Wide(action) if power => action.execute(&mut self.memory),
            Action::Convert(op, value, dest) if power => {
                if let Some(value) = op.apply(value.read(&self.memory) as i16) {
                    dest.write(&mut self.memory, i32::from(value));
                } else {
                    self.memory.operation_error(0xe18, 1);
                }
            }
            Action::BlockMove(source, dest, count) if power => {
                // Native BMOV updates only numeric operand-kind bytes;
                // grouped operands retain their previous half of SORT scratch.
                let old = self.memory.native_numeric_scratch;
                self.memory.native_numeric_scratch =
                    match (source.numeric_kind(), dest.numeric_kind()) {
                        (Some(a), Some(b)) => Some(i16::from_be_bytes([a, b])),
                        (Some(a), None) => {
                            old.map(|v| ((v as u16 & 0xff) | (u16::from(a) << 8)) as i16)
                        }
                        (None, Some(b)) => old.map(|v| ((v as u16 & 0xff00) | u16::from(b)) as i16),
                        (None, None) => old,
                    };
                let count = count.read(&self.memory);
                if (1..=512).contains(&count) {
                    for offset in 0..count as u16 {
                        let more = offset + 1 < count as u16;
                        let value = source.read_bmov_element(
                            &self.memory,
                            offset,
                            more,
                            &mut self.observed_devices,
                        );
                        let result = value.and_then(|value| {
                            dest.write_bmov_element(
                                &mut self.memory,
                                offset,
                                more,
                                value,
                                &mut self.observed_devices,
                            )
                        });
                        if result.is_err() {
                            self.memory.operation_error(0xe1a, 7);
                            break;
                        }
                    }
                }
            }
            Action::Fill(value, dest, count) if power => {
                let kind = u16::from(dest.word_fill_kind());
                self.memory.native_numeric_scratch = self
                    .memory
                    .native_numeric_scratch
                    .map(|old| ((old as u16 & 0xff00) | kind) as i16);
                let value = value.read(&self.memory);
                for offset in 0..*count {
                    dest.element(offset)
                        .expect("validated fill range")
                        .write(&mut self.memory, value);
                }
            }
            Action::Exchange(source, dest) if power => {
                let a = source.read(&self.memory);
                let b = dest.read(&self.memory);
                // The native equality resolver ignores Kn width but uses a
                // different E/F address key from the normal storage reader.
                if source.same_exchange_address(dest) && self.memory.system_bit(1303) {
                    dest.write(&mut self.memory, i32::from((b as u16).swap_bytes() as i16));
                } else {
                    // Preserve the native order when writable groups overlap.
                    dest.write(&mut self.memory, a);
                    source.write(&mut self.memory, b);
                }
            }
            Action::ZoneCompare(low, high, value, _) if power => {
                // Read every source before writing any result bit; packed
                // source groups can overlap the three destination bits.
                let low = low.read(&self.memory);
                // PDF API 11: reversed bounds collapse to the lower
                // operand S1, rather than swapping the operands.
                let high = high.read(&self.memory).max(low);
                let value = value.read(&self.memory);
                for (device, state) in comparison_destinations(d).unwrap().into_iter().zip([
                    value < low,
                    value >= low && value <= high,
                    value > high,
                ]) {
                    self.memory.set_internal(device, state);
                }
            }
            Action::MultiplyTruncated(a, b, dest) if power => {
                let product = a.read(&self.memory) * b.read(&self.memory);
                self.memory.set_system(1022, false);
                dest.write(&mut self.memory, product);
                let grouped = matches!(
                    dest.value(),
                    CalendarValue::Additional(
                        crate::additional_processing::AdditionalValue::Group { .. }
                    )
                );
                if i16::try_from(product).is_err()
                    && (grouped || (!self.memory.system_bit(1067) && !self.memory.system_bit(1068)))
                {
                    self.memory.set_system(1022, true);
                }
            }
            Action::DivideQuotient(a, b, dest) if power => {
                let (a, b) = (a.read(&self.memory), b.read(&self.memory));
                if b == 0 {
                    self.memory.operation_error(0xe19, 5);
                } else {
                    dest.write(&mut self.memory, a / b);
                }
            }
            Action::Multiply(a, b, dest) if power => {
                let value = a.read(&self.memory) * b.read(&self.memory);
                // Full-width MUL, like DMUL, preserves the arithmetic
                // flags. Truncated MUL16 is a different native opcode.
                dest.write_pair(&mut self.memory, [i32::from(value as i16), value >> 16]);
            }
            Action::Divide(a, b, dest) if power => {
                let a = a.read(&self.memory);
                let b = b.read(&self.memory);
                if b != 0 {
                    dest.write_pair(&mut self.memory, [a / b, a % b]);
                } else {
                    self.memory.operation_error(0xe19, 5);
                }
            }
            Action::Compare(a, b, _) if power => {
                // Snapshot both values before writes even for Kn destination
                // aliases. CalendarValue supplies the selected signed width.
                let a = a.read(&self.memory);
                let b = b.read(&self.memory);
                for (device, value) in
                    comparison_destinations(d)
                        .unwrap()
                        .into_iter()
                        .zip([a > b, a == b, a < b])
                {
                    self.memory.set_internal(device, value);
                }
            }
            Action::Out(_) => {
                if !crate::sfc::SfcState::transition(
                    &mut self.memory,
                    &mut self.control,
                    d,
                    false,
                    power,
                ) {
                    self.memory.set_bit(d, power);
                }
            }
            Action::Set(_) if power => {
                if !crate::sfc::SfcState::transition(
                    &mut self.memory,
                    &mut self.control,
                    d,
                    true,
                    power,
                ) {
                    self.memory.set_bit(d, true);
                }
            }
            Action::Reset(_) if d.area == Area::T => {
                crate::special_timers::TimerState::visit_reset(&mut self.memory, d, power);
            }
            Action::Reset(_) if d.area == Area::C && d.index < 200 => {
                crate::word_counter::WordCounterState::visit_reset(&mut self.memory, d, power);
            }
            Action::Reset(_) if d.area == Area::C && d.index <= 254 => {
                crate::counter::CounterState::visit_reset(&mut self.memory, d, power);
            }
            Action::Reset(_) if power => {
                self.memory.reset(d);
            }
            Action::Timer(_, preset) => {
                TimerAction::Timer {
                    timer: d,
                    preset: crate::special_timers::TimerPreset::Constant(*preset as i16),
                    contact: false,
                }
                .execute(&mut self.memory, power, pc);
            }
            Action::Counter(_, preset) => {
                crate::word_counter::WordCounterAction {
                    counter: d,
                    preset: crate::word_counter::WordCounterPreset::Constant(*preset as i16),
                }
                .execute(&mut self.memory, power, pc);
            }
            Action::WordCounter(action) => action.execute(&mut self.memory, power, pc),
            Action::NumericReset(action) => action.execute(&mut self.memory, power),
            Action::Move(v, dest) if power => {
                let v = v.read(&self.memory);
                dest.write(&mut self.memory, v);
            }
            Action::Add(a, b, dest) if power => {
                let (a, b) = (a.read(&self.memory) as i16, b.read(&self.memory) as i16);
                let v = es2_add(a, b);
                self.memory.arithmetic_flags(
                    i32::from(a),
                    i32::from(b),
                    i32::from(a.wrapping_add(b)),
                    false,
                    i32::from(v),
                );
                dest.write(&mut self.memory, i32::from(v));
            }
            Action::Sub(a, b, dest) if power => {
                let (a, b) = (a.read(&self.memory) as i16, b.read(&self.memory) as i16);
                let v = es2_sub(a, b);
                self.memory.arithmetic_flags(
                    i32::from(a),
                    i32::from(b),
                    i32::from(a.wrapping_sub(b)),
                    true,
                    i32::from(v),
                );
                dest.write(&mut self.memory, i32::from(v));
            }
            Action::Unary(op, dest) if power => {
                let value = op.apply(dest.read(&self.memory) as i16);
                dest.write(&mut self.memory, i32::from(value));
            }
            Action::Complement(a, dest) if power => {
                let value = !(a.read(&self.memory) as i16);
                dest.write(&mut self.memory, i32::from(value));
            }
            Action::Logic(op, a, b, dest) if power => {
                // Packed destinations may overlap either source group.
                let (a, b) = (a.read(&self.memory) as i16, b.read(&self.memory) as i16);
                dest.write(&mut self.memory, i32::from(op.apply(a, b)));
            }
            _ => {}
        }
        crate::pulse_output::PulseState::service_controls(&mut self.memory);
        crate::motion_basic::MotionBasicState::service_controls(&mut self.memory);
        crate::motion_extended::MotionExtendedState::service_controls(&mut self.memory);
        crate::serial_io::SerialBus::service_controls(&mut self.memory);
        crate::ethernet_raw::RawEthernetState::service_controls(&mut self.memory);
        crate::ethernet_config::EthernetConfigState::service_controls(&mut self.memory);
        crate::coordinated_motion::CoordinatedState::service_controls(&mut self.memory);
        crate::analog_knob::KnobState::service(&mut self.memory);
    }
}
