//! Read-only qualification of a paused RAND/DRAND header in the two exact
//! current COMMGR images. Native state is evidence, never local-model input.
use super::Session;
use crate::random_tail::RandomGateState;

#[cfg(any(windows, test))]
use super::{COMMGR_ES2_SHA256, COMMGR_SS2_SHA256};

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GateAddresses {
    logic: usize,
    pc: usize,
    master: usize,
    pulse_bitmap: usize,
}

#[cfg(any(windows, test))]
fn gate_addresses(digest: &str) -> Result<GateAddresses, String> {
    match digest {
        COMMGR_ES2_SHA256 => Ok(GateAddresses {
            logic: 0x553f2c,
            pc: 0x553f2e,
            master: 0x553f44,
            pulse_bitmap: 0x554004,
        }),
        COMMGR_SS2_SHA256 => Ok(GateAddresses {
            logic: 0x535ed0,
            pc: 0x535ed2,
            master: 0x535ee8,
            pulse_bitmap: 0x535fa8,
        }),
        _ => Err(
            "Inconclusive operand native gate: executable digest has no qualified field map".into(),
        ),
    }
}

#[cfg(any(windows, test))]
fn pulse_location(bitmap: usize, header_pc: u16) -> (usize, u8) {
    (bitmap + usize::from(header_pc >> 3), 1 << (header_pc & 7))
}

#[cfg(any(windows, test))]
fn qualify_state(
    expected: RandomGateState,
    observed: RandomGateState,
    pulse: bool,
) -> Result<bool, String> {
    // Equal body-active booleans are insufficient: old P changes DRAND's
    // disabled PC disposition, and upper L bits affect operand-tail controls.
    if expected != observed {
        return Err(format!(
            "Inconclusive operand native gate at PC {}: expected {expected:?}, observed {observed:?}; no operand access or native STEP is qualified",
            expected.pc
        ));
    }
    Ok(observed.body_active(pulse))
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy)]
enum GateKind {
    Random,
    Calendar,
    Rtc,
    Segment,
    Checksum,
    Text,
    Additional,
}
#[cfg(any(windows, test))]
fn qualify_header_for(
    program: &crate::plc::Program,
    encoded: &crate::es2_encoder::EncodedProgram,
    pc: u16,
    pulse: bool,
    kind: GateKind,
) -> Result<(), String> {
    use crate::{execution_plan::PlanOp, plc::Action};
    let index =
        encoded.execution_plan.by_pc.get(&pc).copied().ok_or(
            "Inconclusive operand native gate: PC is not an uploaded instruction boundary",
        )?;
    let node = encoded
        .execution_plan
        .nodes
        .get(index)
        .ok_or("Inconclusive operand native gate: invalid instruction-plan index")?;
    let PlanOp::Action {
        network, action, ..
    } = &node.op
    else {
        return Err("Inconclusive operand native gate: PC is not an uploaded action header".into());
    };
    let action = program
        .networks
        .get(*network)
        .and_then(|network| network.actions.get(*action))
        .ok_or("Inconclusive operand native gate: action has no source binding")?;
    let (action, source_pulse) = match action {
        Action::Pulse(inner) => (inner.as_ref(), true),
        action => (action, false),
    };
    let (normal, length) = match kind {
        GateKind::Random => {
            let wide = match action {
                Action::Random(a) => a.wide(),
                Action::IndexedRandom(a) => a.wide(),
                _ => return Err("Native random gate action binding mismatch".into()),
            };
            (
                if wide { 0x5501 } else { 0x5401 },
                if wide { 13 } else { 7 },
            )
        }
        GateKind::Calendar => match action {
            Action::IndexedCalendarTime(a) => (
                (a.native_words()?[0] & !0x10),
                usize::from(a.nominal_words()),
            ),
            Action::Time(a @ crate::time_ops::TimeAction::Arithmetic { .. }) => (a.header(), 7),
            _ => return Err("Native calendar gate action binding mismatch".into()),
        },
        GateKind::Rtc => match action {
            Action::IndexedRtc(a) => (a.native_words()?[0] & !0x10, 3),
            Action::Rtc(a) => (a.header(), 3),
            _ => return Err("Native RTC gate action binding mismatch".into()),
        },
        GateKind::Additional => match action {
            Action::IndexedAdditional(a) => (a.header() & !0x10, usize::from(a.nominal_words())),
            Action::Additional(
                a @ (crate::additional_processing::AdditionalAction::WordSum { .. }
                | crate::additional_processing::AdditionalAction::Scale { .. }
                | crate::additional_processing::AdditionalAction::ScalePoints { .. }
                | crate::additional_processing::AdditionalAction::CompareTable { .. }),
            ) => {
                let a = crate::indexed_additional::IndexedAdditionalAction::from_legacy(a)?;
                (a.header() & !0x10, usize::from(a.nominal_words()))
            }
            _ => return Err("Native additional gate action binding mismatch".into()),
        },
        GateKind::Text => match action {
            Action::IndexedText(a) => (a.native_words()?[0] & !0x10, 7),
            Action::Text(
                a @ (crate::text_encoding::TextAction::Ascii { .. }
                | crate::text_encoding::TextAction::Hex { .. }
                | crate::text_encoding::TextAction::Check { .. }),
            ) => (
                crate::indexed_text::IndexedTextAction::from_legacy(a)?.native_words()?[0] & !0x10,
                7,
            ),
            _ => return Err("Native text gate action binding mismatch".into()),
        },
        GateKind::Checksum => match action {
            Action::IndexedChecksum(a) => (a.native_words()?[0] & !0x10, 7),
            Action::Text(
                a @ (crate::text_encoding::TextAction::Lrc { .. }
                | crate::text_encoding::TextAction::Crc { .. }),
            ) => (
                crate::indexed_checksum::IndexedChecksumAction::from_legacy(a)?.native_words()?[0]
                    & !0x10,
                7,
            ),
            _ => return Err("Native checksum gate action binding mismatch".into()),
        },
        GateKind::Segment => match action {
            Action::IndexedSegment(a) => (a.native_words()?[0] & !0x10, 5),
            _ => return Err("Native SEGD gate action binding mismatch".into()),
        },
    };
    let header = normal | if pulse { 0x10 } else { 0 };
    let start = usize::from(pc);
    let end = start + length;
    if matches!(kind, GateKind::Additional)
        && (usize::from(node.fallthrough) != usize::from(pc) + length
            || encoded.execution_plan.operand_tail_owners.contains_key(&pc))
    {
        return Err("Native additional gate requires original WORD lengths without tails".into());
    }
    if matches!(kind, GateKind::Text)
        && (node.fallthrough != pc + 7
            || encoded.execution_plan.operand_tail_owners.contains_key(&pc))
    {
        return Err("Native text gate requires seven original WORDs without tails".into());
    }
    if matches!(kind, GateKind::Checksum)
        && (node.fallthrough != pc + 7
            || encoded.execution_plan.operand_tail_owners.contains_key(&pc))
    {
        return Err("Native checksum gate requires seven original WORDs without tails".into());
    }
    if matches!(kind, GateKind::Segment)
        && (node.fallthrough != pc + 5
            || encoded.execution_plan.operand_tail_owners.contains_key(&pc))
    {
        return Err("Native SEGD gate requires five original WORDs without operand tails".into());
    }
    if matches!(kind, GateKind::Rtc)
        && (node.fallthrough != pc + 3
            || encoded.execution_plan.operand_tail_owners.contains_key(&pc))
    {
        return Err(
            "Native RTC gate requires its original three WORDs without operand tails".into(),
        );
    }
    if matches!(kind, GateKind::Calendar)
        && matches!(normal, 0x6401 | 0x6601)
        && (node.fallthrough != pc + 3
            || encoded.execution_plan.operand_tail_owners.get(&pc)
                != Some(&crate::random_tail::OperandTailOwner::CalendarTime))
    {
        return Err("Native calendar gate tail ownership mismatch".into());
    }
    if node.pc != pc
        || source_pulse != pulse
        || encoded.words.get(start) != Some(&header)
        || end > encoded.words.len()
    {
        return Err(
            "Inconclusive operand native gate: header, width or pulse binding mismatch".into(),
        );
    }
    Ok(())
}

impl Session {
    #[cfg(windows)]
    pub(super) fn qualify_additional_native_gate(
        &mut self,
        expected: RandomGateState,
        pulse: bool,
    ) -> Result<bool, String> {
        self.qualify_owned_native_gate(expected, pulse, GateKind::Additional)
    }
    #[cfg(not(windows))]
    pub(super) fn qualify_additional_native_gate(
        &mut self,
        _: RandomGateState,
        _: bool,
    ) -> Result<bool, String> {
        Err("Native additional header gate requires Windows".into())
    }
    #[cfg(windows)]
    pub(super) fn qualify_text_native_gate(
        &mut self,
        expected: RandomGateState,
        pulse: bool,
    ) -> Result<bool, String> {
        self.qualify_owned_native_gate(expected, pulse, GateKind::Text)
    }
    #[cfg(not(windows))]
    pub(super) fn qualify_text_native_gate(
        &mut self,
        _: RandomGateState,
        _: bool,
    ) -> Result<bool, String> {
        Err("Native text header gate requires Windows".into())
    }
    #[cfg(windows)]
    pub(super) fn qualify_checksum_native_gate(
        &mut self,
        expected: RandomGateState,
        pulse: bool,
    ) -> Result<bool, String> {
        self.qualify_owned_native_gate(expected, pulse, GateKind::Checksum)
    }
    #[cfg(not(windows))]
    pub(super) fn qualify_checksum_native_gate(
        &mut self,
        _: RandomGateState,
        _: bool,
    ) -> Result<bool, String> {
        Err("Native checksum header gate requires Windows".into())
    }
    #[cfg(windows)]
    pub(super) fn qualify_segment_native_gate(
        &mut self,
        expected: RandomGateState,
        pulse: bool,
    ) -> Result<bool, String> {
        self.qualify_owned_native_gate(expected, pulse, GateKind::Segment)
    }
    #[cfg(not(windows))]
    pub(super) fn qualify_segment_native_gate(
        &mut self,
        _: RandomGateState,
        _: bool,
    ) -> Result<bool, String> {
        Err("Native SEGD header gate requires Windows".into())
    }
    #[cfg(windows)]
    pub(super) fn qualify_rtc_native_gate(
        &mut self,
        expected: RandomGateState,
        pulse: bool,
    ) -> Result<bool, String> {
        self.qualify_owned_native_gate(expected, pulse, GateKind::Rtc)
    }
    #[cfg(not(windows))]
    pub(super) fn qualify_rtc_native_gate(
        &mut self,
        _: RandomGateState,
        _: bool,
    ) -> Result<bool, String> {
        Err("Native RTC header gate requires Windows".into())
    }
    #[cfg(windows)]
    pub(super) fn qualify_random_native_gate(
        &mut self,
        expected: RandomGateState,
        pulse: bool,
    ) -> Result<bool, String> {
        self.qualify_owned_native_gate(expected, pulse, GateKind::Random)
    }
    #[cfg(windows)]
    pub(super) fn qualify_calendar_native_gate(
        &mut self,
        expected: RandomGateState,
        pulse: bool,
    ) -> Result<bool, String> {
        self.qualify_owned_native_gate(expected, pulse, GateKind::Calendar)
    }
    #[cfg(windows)]
    fn qualify_owned_native_gate(
        &mut self,
        expected: RandomGateState,
        pulse: bool,
        kind: GateKind,
    ) -> Result<bool, String> {
        use super::{PC, owns_listener};
        use std::{os::windows::io::AsRawHandle, sync::atomic::Ordering};

        let addresses = gate_addresses(&self.digest)?;
        if self.faulted || self.online_active || self.cancel.load(Ordering::Relaxed) {
            return Err(
                "Inconclusive operand native gate: session is not available for paused stepping"
                    .into(),
            );
        }
        let program = self.program.as_ref().ok_or("No uploaded random program")?;
        let encoded = self.encoded.as_ref().ok_or("No encoded random program")?;
        self.target
            .validate_program_word_count(encoded.words.len())?;
        qualify_header_for(program, encoded, expected.pc, pulse, kind)?;
        let (handle, pid) = {
            let native = self.native.as_mut().ok_or("No owned native process")?;
            let child = native
                ._child
                .0
                .as_mut()
                .ok_or("Owned process handle unavailable")?;
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Err("Inconclusive operand native gate: owned simulator has exited".into());
            }
            (child.as_raw_handle(), child.id())
        };
        if !owns_listener(pid, self.port)? {
            return Err(
                "Inconclusive operand native gate: listener is not owned by this child".into(),
            );
        }
        windows_read::qualify_image_base(handle)?;
        if self.word_read(PC)? != expected.pc {
            return Err(
                "Inconclusive operand native gate: protocol PC is not the expected paused header"
                    .into(),
            );
        }

        let before = u16::from_le_bytes(windows_read::read::<2>(handle, addresses.pc)?);
        if before != expected.pc {
            return Err(
                "Inconclusive operand native gate: native PC changed before field capture".into(),
            );
        }
        let logic_word = u16::from_le_bytes(windows_read::read::<2>(handle, addresses.logic)?);
        let master_on = windows_read::read::<1>(handle, addresses.master)?[0] & 1 != 0;
        let pulse_latched = if pulse {
            // This header was validated inside the already uploaded program;
            // no claim about any larger pulse bitmap extent is required.
            let (address, mask) = pulse_location(addresses.pulse_bitmap, expected.pc);
            windows_read::read::<1>(handle, address)?[0] & mask != 0
        } else {
            false
        };
        let after = u16::from_le_bytes(windows_read::read::<2>(handle, addresses.pc)?);
        if after != before {
            return Err(
                "Inconclusive operand native gate: native PC moved during field capture".into(),
            );
        }

        let child = self
            .native
            .as_mut()
            .and_then(|n| n._child.0.as_mut())
            .ok_or("Owned process handle disappeared during random gate capture")?;
        if child.id() != pid
            || child.try_wait().map_err(|e| e.to_string())?.is_some()
            || !owns_listener(pid, self.port)?
        {
            return Err(
                "Inconclusive operand native gate: owned listener changed during capture".into(),
            );
        }
        if self.word_read(PC)? != expected.pc {
            return Err(
                "Inconclusive operand native gate: protocol PC changed after field capture".into(),
            );
        }
        qualify_state(
            expected,
            RandomGateState {
                pc: after,
                logic_word,
                master_on,
                pulse_latched,
            },
            pulse,
        )
    }

    #[cfg(not(windows))]
    pub(super) fn qualify_random_native_gate(
        &mut self,
        _: RandomGateState,
        _: bool,
    ) -> Result<bool, String> {
        Err("Inconclusive operand native gate: owned process observation requires Windows".into())
    }
}

#[cfg(windows)]
pub(super) mod windows_read {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReadProcessMemory(
            process: *mut c_void,
            address: *const c_void,
            buffer: *mut c_void,
            size: usize,
            read: *mut usize,
        ) -> i32;
        fn K32EnumProcessModulesEx(
            process: *mut c_void,
            modules: *mut *mut c_void,
            bytes: u32,
            needed: *mut u32,
            filter: u32,
        ) -> i32;
    }

    pub(in crate::es2_bridge) fn qualify_image_base(handle: *mut c_void) -> Result<(), String> {
        let mut first = std::ptr::null_mut();
        let mut needed = 0_u32;
        let bytes = std::mem::size_of::<*mut c_void>() as u32;
        // The first module is the owned executable. LIST_MODULES_ALL supports
        // this 64-bit host observing its 32-bit child. Relocated images remain
        // unqualified; the static field map must never be guessed or rebased.
        let ok = unsafe { K32EnumProcessModulesEx(handle, &mut first, bytes, &mut needed, 3) };
        if ok == 0 {
            return Err(format!(
                "Inconclusive operand native gate: image-base query failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        if needed < bytes || first as usize != 0x400000 {
            return Err(format!(
                "Inconclusive operand native gate: unqualified loaded image base {:?}",
                first
            ));
        }
        Ok(())
    }

    pub(in crate::es2_bridge) fn read<const N: usize>(
        handle: *mut c_void,
        address: usize,
    ) -> Result<[u8; N], String> {
        let mut bytes = [0_u8; N];
        let mut transferred = 0_usize;
        let ok = unsafe {
            ReadProcessMemory(
                handle,
                address as *const c_void,
                bytes.as_mut_ptr().cast(),
                N,
                &mut transferred,
            )
        };
        if ok == 0 {
            return Err(format!(
                "Inconclusive operand native gate: read at {address:#x} failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        if transferred != N {
            return Err(format!(
                "Inconclusive operand native gate: short read at {address:#x}, {transferred}/{N} bytes"
            ));
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn additional_gate_binds_all_fourteen_headers_lengths_pulse_and_profile() {
        for (base, body) in [
            ("WSUM", "D0E1 D40F7 D20E0"),
            ("DWSUM", "D0E1 D40F7 D20E0"),
            ("SCAL", "D0E1 D10F7 D15E0 D20F0"),
            ("SCLP", "D0E1 D40F7 D20E0"),
            ("DSCLP", "D0E1 D40F7 D20E0"),
            ("CMPT", "D0E1 D10F7 D40E0 D20F0"),
            ("DCMPT", "D0E1 D10F7 D40E0 D20F0"),
        ] {
            for pulse in [false, true] {
                let name = format!("{base}{}", if pulse { "P" } else { "" });
                let p = crate::plc::compile_il(&format!("LD X0\n{name} {body}\nEND")).unwrap();
                let e = crate::es2_encoder::encode(&p).unwrap();
                assert!(qualify_header_for(&p, &e, 1, pulse, GateKind::Additional).is_ok());
                assert!(qualify_header_for(&p, &e, 1, !pulse, GateKind::Additional).is_err());
                assert!(qualify_header_for(&p, &e, 1, pulse, GateKind::Text).is_err());
                let mut bad = e;
                bad.execution_plan.nodes[1].fallthrough -= 1;
                assert!(qualify_header_for(&p, &bad, 1, pulse, GateKind::Additional).is_err());
                for digest in [COMMGR_ES2_SHA256, COMMGR_SS2_SHA256] {
                    assert!(super::super::validate_additional_native_profile(&p, digest).is_ok());
                }
                assert!(super::super::validate_additional_native_profile(&p, "unknown").is_err());
            }
        }
        for (name, args) in [
            ("WSUM", "D0 K2 D20"),
            ("SCAL", "D0 K1000 K2 D20"),
            ("SCLP", "D0 D10 D20"),
            ("DSCLP", "D0 D10 D20"),
            ("CMPT", "D0 D10 K2 D20"),
            ("DCMPT", "D0 D10 K2 D20"),
        ] {
            let mut p = crate::plc::compile_il(&format!("LD X0\n{name} {args}\nEND")).unwrap();
            let args = args
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            p.networks[0].actions[0] = crate::plc::Action::Additional(
                crate::additional_processing::AdditionalAction::parse(name, &args).unwrap(),
            );
            assert!(p.requires_vm());
            let e = crate::es2_encoder::encode(&p).unwrap();
            assert!(qualify_header_for(&p, &e, 1, false, GateKind::Additional).is_ok());
            assert!(super::super::validate_additional_native_profile(&p, "unknown").is_err());
        }
    }
    #[test]
    fn text_gate_checks_six_headers_aliases_and_original_seven_words() {
        for name in [
            "ASCI", "ASCIP", "ASCII", "ASCIIP", "HEX", "HEXP", "CCD", "CCDP",
        ] {
            let pulse = name.ends_with('P');
            let count = if name.starts_with("CCD") { "D40" } else { "K3" };
            let p =
                crate::plc::compile_il(&format!("LD X0\n{name} K4X100E1 K2M128F7 {count}\nEND"))
                    .unwrap();
            let e = crate::es2_encoder::encode(&p).unwrap();
            assert!(qualify_header_for(&p, &e, 1, pulse, GateKind::Text).is_ok());
            assert!(qualify_header_for(&p, &e, 1, !pulse, GateKind::Text).is_err());
            assert!(qualify_header_for(&p, &e, 1, pulse, GateKind::Checksum).is_err());
            let mut bad = e;
            bad.execution_plan.nodes[1].fallthrough = 7;
            assert!(qualify_header_for(&p, &bad, 1, pulse, GateKind::Text).is_err());
            for digest in [COMMGR_ES2_SHA256, COMMGR_SS2_SHA256] {
                assert!(super::super::validate_text_native_profile(&p, digest).is_ok());
            }
            assert!(super::super::validate_text_native_profile(&p, "unknown").is_err());
        }
        for name in ["ASCI", "HEX", "CCD"] {
            let mut p = crate::plc::compile_il(&format!("LD X0\n{name} D0 D20 K2\nEND")).unwrap();
            let args = ["D0", "D20", "K2"].map(str::to_owned);
            p.networks[0].actions[0] = crate::plc::Action::Text(
                crate::text_encoding::TextAction::parse(name, &args).unwrap(),
            );
            let e = crate::es2_encoder::encode(&p).unwrap();
            assert!(qualify_header_for(&p, &e, 1, false, GateKind::Text).is_ok());
        }
    }
    #[test]
    fn checksum_gate_checks_all_four_headers_and_seven_word_boundaries() {
        for name in ["LRC", "LRCP", "CRC", "CRCP"] {
            for operands in ["D10E1 D30F0 D100F7", "D10 K2 D100"] {
                let pulse = name.ends_with('P');
                let p = crate::plc::compile_il(&format!("LD X0\n{name} {operands}\nEND")).unwrap();
                let e = crate::es2_encoder::encode(&p).unwrap();
                assert!(qualify_header_for(&p, &e, 1, pulse, GateKind::Checksum).is_ok());
                assert!(qualify_header_for(&p, &e, 1, !pulse, GateKind::Checksum).is_err());
                assert!(qualify_header_for(&p, &e, 1, pulse, GateKind::Segment).is_err());
                let mut bad = e;
                bad.execution_plan.nodes[1].fallthrough = 7;
                assert!(qualify_header_for(&p, &bad, 1, pulse, GateKind::Checksum).is_err());
                for digest in [COMMGR_ES2_SHA256, COMMGR_SS2_SHA256] {
                    assert!(super::super::validate_checksum_native_profile(&p, digest).is_ok());
                }
                assert!(super::super::validate_checksum_native_profile(&p, "unknown").is_err());
            }
        }
    }
    #[test]
    fn segment_header_gate_checks_five_original_words_and_both_pulse_forms() {
        for name in ["SEGD", "SEGDP"] {
            let pulse = name.ends_with('P');
            let p = crate::plc::compile_il(&format!("LD X0\n{name} D10E1 K1M128F7\nEND")).unwrap();
            let e = crate::es2_encoder::encode(&p).unwrap();
            assert!(qualify_header_for(&p, &e, 1, pulse, GateKind::Segment).is_ok());
            assert!(qualify_header_for(&p, &e, 1, !pulse, GateKind::Segment).is_err());
            assert!(qualify_header_for(&p, &e, 1, pulse, GateKind::Rtc).is_err());
            let mut bad = e;
            bad.execution_plan.nodes[1].fallthrough = 4;
            assert!(qualify_header_for(&p, &bad, 1, pulse, GateKind::Segment).is_err());
        }
    }
    #[test]
    fn segment_native_profile_guard_keeps_plain_legacy_segment_available() {
        let indexed = crate::plc::compile_il("LD X0\nSEGD D0E0 D10\nEND").unwrap();
        let plain = crate::plc::compile_il("LD X0\nSEGD D0 D10\nEND").unwrap();
        for hash in [COMMGR_ES2_SHA256, COMMGR_SS2_SHA256] {
            assert!(super::super::validate_segment_native_profile(&indexed, hash).is_ok());
        }
        for hash in [
            super::super::TARGET_SHA256,
            super::super::SS2_SHA256,
            "unknown",
        ] {
            assert!(super::super::validate_segment_native_profile(&indexed, hash).is_err());
            assert!(super::super::validate_segment_native_profile(&plain, hash).is_ok());
        }
    }

    #[test]
    fn rtc_gate_binds_all_four_source_forms_without_operand_tails() {
        for name in ["TRD", "TWR"] {
            for pulse in [false, true] {
                for operand in ["D100", "T100E1", "C100F7"] {
                    let p = crate::plc::compile_il(&format!(
                        "LD X0\n{name}{} {operand}\nEND",
                        if pulse { "P" } else { "" }
                    ))
                    .unwrap();
                    let e = crate::es2_encoder::encode(&p).unwrap();
                    assert!(qualify_header_for(&p, &e, 1, pulse, GateKind::Rtc).is_ok());
                    assert!(qualify_header_for(&p, &e, 1, !pulse, GateKind::Rtc).is_err());
                    assert!(qualify_header_for(&p, &e, 1, pulse, GateKind::Calendar).is_err());
                    let mut bad = e.clone();
                    bad.execution_plan.nodes[1].fallthrough = 8;
                    assert!(qualify_header_for(&p, &bad, 1, pulse, GateKind::Rtc).is_err());
                    let mut bad = e;
                    bad.execution_plan
                        .operand_tail_owners
                        .insert(1, crate::random_tail::OperandTailOwner::CalendarTime);
                    assert!(qualify_header_for(&p, &bad, 1, pulse, GateKind::Rtc).is_err());
                }
            }
        }
    }

    #[test]
    fn maps_only_two_exact_current_images() {
        assert_eq!(
            gate_addresses(COMMGR_ES2_SHA256).unwrap(),
            GateAddresses {
                logic: 0x553f2c,
                pc: 0x553f2e,
                master: 0x553f44,
                pulse_bitmap: 0x554004
            }
        );
        assert_eq!(
            gate_addresses(COMMGR_SS2_SHA256).unwrap(),
            GateAddresses {
                logic: 0x535ed0,
                pc: 0x535ed2,
                master: 0x535ee8,
                pulse_bitmap: 0x535fa8
            }
        );
        for digest in [
            super::super::TARGET_SHA256,
            super::super::SS2_SHA256,
            "",
            "unknown",
            "55707513",
        ] {
            assert!(gate_addresses(digest).is_err());
        }
    }

    #[test]
    fn pulse_byte_and_mask_use_actual_word_header_pc() {
        assert_eq!(pulse_location(0x554004, 0), (0x554004, 1));
        assert_eq!(pulse_location(0x554004, 7), (0x554004, 0x80));
        assert_eq!(pulse_location(0x554004, 8), (0x554005, 1));
        assert_eq!(pulse_location(0x535fa8, 0x123), (0x535fcc, 8));
        assert_eq!(pulse_location(0x535fa8, 0xffff), (0x537fa7, 0x80));
    }

    #[test]
    fn full_logic_master_history_and_pc_must_match_even_if_both_bodies_skip() {
        let expected = RandomGateState {
            pc: 17,
            logic_word: 0,
            master_on: true,
            pulse_latched: false,
        };
        assert_eq!(qualify_state(expected, expected, true), Ok(false));
        for observed in [
            RandomGateState {
                logic_word: 0x100,
                ..expected
            },
            RandomGateState {
                master_on: false,
                ..expected
            },
            RandomGateState {
                pulse_latched: true,
                ..expected
            },
            RandomGateState { pc: 18, ..expected },
        ] {
            assert!(!observed.body_active(true));
            assert!(qualify_state(expected, observed, true).is_err());
        }
        let active = RandomGateState {
            logic_word: 0x101,
            ..expected
        };
        assert_eq!(qualify_state(active, active, false), Ok(true));
        assert_eq!(qualify_state(active, active, true), Ok(true));
        let held = RandomGateState {
            pulse_latched: true,
            ..active
        };
        assert_eq!(qualify_state(held, held, true), Ok(false));
        assert!(qualify_state(active, held, true).is_err());
    }

    #[test]
    fn only_uploaded_rand_headers_qualify_not_tail_or_following_instruction() {
        for op in ["RAND", "RANDP", "DRAND", "DRANDP"] {
            let program =
                crate::plc::compile_il(&format!("LD X0\n{op} K7 K7 D0E0\nOUT Y0\nEND")).unwrap();
            let encoded = crate::es2_encoder::encode(&program).unwrap();
            let pc = encoded.action_pc[0];
            let pulse = op.ends_with('P');
            assert!(qualify_header(&program, &encoded, pc, pulse).is_ok());
            assert!(qualify_header(&program, &encoded, pc, !pulse).is_err());
            assert!(qualify_header(&program, &encoded, pc + 1, pulse).is_err());
            assert!(qualify_header(&program, &encoded, encoded.action_pc[1], pulse).is_err());
            assert!(qualify_header(&program, &encoded, u16::MAX, pulse).is_err());
        }
    }
}

#[cfg(test)]
fn qualify_header(
    program: &crate::plc::Program,
    encoded: &crate::es2_encoder::EncodedProgram,
    pc: u16,
    pulse: bool,
) -> Result<(), String> {
    qualify_header_for(program, encoded, pc, pulse, GateKind::Random)
}
#[cfg(test)]
mod calendar_header_tests {
    use super::*;
    #[test]
    fn calendar_gate_binds_all_headers_pulse_sites_and_tail_owners() {
        for (name, args) in [
            ("TCMP", "K1 K2 K3 D0E0 M80F0"),
            ("TZCP", "D0E0 D10 D20 M80F0"),
            ("TADD", "D0 D10 D20"),
            ("TSUB", "D0 D10 D20"),
        ] {
            for pulse in [false, true] {
                let p = crate::plc::compile_il(&format!(
                    "LD X0\n{name}{} {args}\nEND",
                    if pulse { "P" } else { "" }
                ))
                .unwrap();
                let e = crate::es2_encoder::encode(&p).unwrap();
                assert!(qualify_header_for(&p, &e, 1, pulse, GateKind::Calendar).is_ok());
                assert!(qualify_header_for(&p, &e, 1, !pulse, GateKind::Calendar).is_err());
                assert!(qualify_header_for(&p, &e, 1, pulse, GateKind::Random).is_err());
                let mut bad = e.clone();
                bad.words[1] ^= 0x100;
                assert!(qualify_header_for(&p, &bad, 1, pulse, GateKind::Calendar).is_err());
                if matches!(name, "TADD" | "TSUB") {
                    let mut bad = e;
                    bad.execution_plan.operand_tail_owners.clear();
                    assert!(qualify_header_for(&p, &bad, 1, pulse, GateKind::Calendar).is_err());
                }
            }
        }
    }
    #[test]
    fn calendar_native_capability_is_distinct_from_documented_offline_policy() {
        use crate::calendar_tail::CalendarTailProfile as P;
        assert_eq!(
            super::super::calendar_tail_profile(COMMGR_ES2_SHA256),
            P::CurrentCommgr
        );
        assert_eq!(
            super::super::calendar_tail_profile(COMMGR_SS2_SHA256),
            P::CurrentCommgr
        );
        for digest in [
            super::super::TARGET_SHA256,
            super::super::SS2_SHA256,
            "unknown",
        ] {
            assert_eq!(
                super::super::calendar_tail_profile(digest),
                P::UnqualifiedNative
            );
        }
    }
}
#[cfg(not(windows))]
impl Session {
    pub(super) fn qualify_calendar_native_gate(
        &mut self,
        _: RandomGateState,
        _: bool,
    ) -> Result<bool, String> {
        Err("Native calendar header gate requires Windows".into())
    }
}
