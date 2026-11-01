//! RAND/DRAND numeric body reconstructed from the installed ES2 handler.
//!
//! The original also mutates its 16-bit state from a host timing routine.
//! Standalone execution uses a reproducible seed of one instead: this matches
//! the recovered body for a controlled state, not independent native streams.
use crate::bit_index_operand::BitIndexValue;
use crate::mask_random_operand::MaskRandomDestination;
use crate::plc::{Device, Memory};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct RandomAction {
    pub lower: BitIndexValue,
    pub upper: BitIndexValue,
    pub dest: MaskRandomDestination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RandomDomainError;

impl RandomAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !matches!(name, "RAND" | "DRAND") || args.len() != 3 {
            return Err(format!("{name}: RAND/DRAND üç operand gerektirir"));
        }
        let wide = name == "DRAND";
        Ok(Self {
            lower: BitIndexValue::parse(&args[0], wide)?,
            upper: BitIndexValue::parse(&args[1], wide)?,
            dest: MaskRandomDestination::parse(&args[2], wide)?,
        })
    }
    pub fn wide(&self) -> bool {
        self.lower.wide()
    }
    pub fn name(&self) -> &'static str {
        if self.wide() { "DRAND" } else { "RAND" }
    }
    pub fn header(&self) -> u16 {
        if self.wide() { 0x5501 } else { 0x5401 }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut words = vec![self.header()];
        words.extend(crate::es2_encoder::bit_index_operand(&self.lower)?);
        words.extend(crate::es2_encoder::bit_index_operand(&self.upper)?);
        words.extend(crate::es2_encoder::mask_random_destination_operand(
            &self.dest,
        )?);
        Ok(words)
    }
    pub fn dest(&self) -> Device {
        self.dest.base()
    }
    pub fn label(&self) -> String {
        format!(
            "{} {} {} {}",
            self.name(),
            self.lower,
            self.upper,
            self.dest
        )
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        self.dest.read(memory).to_string()
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.lower.devices(out);
        self.upper.devices(out);
        self.dest.devices(out);
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }
    /// Called only when the outer instruction/pulse has enabled power.
    pub fn execute(&self, m: &mut Memory) {
        // Snapshot both bounds before changing state or an aliased destination.
        let lower = self.lower.read(m);
        let upper = self.upper.read(m);
        if lower < 0 || upper < 0 {
            m.operation_error(0x0e1a, if self.wide() { 5 } else { 3 });
            return;
        }
        let state = m.random_state.get_or_insert(1);
        match sample_with_state(lower, upper, self.wide(), state) {
            Ok(value) => self.dest.write(m, value),
            Err(RandomDomainError) => {
                m.operation_error(0x0e1a, if self.wide() { 5 } else { 3 });
            }
        }
    }
}

/// Execute the recovered body with caller-controlled native 16-bit state.
///
/// Negative bounds leave the state unchanged. Reversed nonnegative bounds
/// update state before returning an error, as does the installed handler.
pub fn sample_with_state(
    lower: i32,
    upper: i32,
    wide: bool,
    state: &mut u16,
) -> Result<i32, RandomDomainError> {
    if lower < 0 || upper < 0 {
        return Err(RandomDomainError);
    }
    *state = state.wrapping_add(match *state & 3 {
        3 => 100,
        2 => 200,
        1 => 300,
        _ => 0,
    });
    let seed = if wide {
        (u32::from(*state & 0x7fff) << 16) | u32::from(state.wrapping_add(0x5789))
    } else {
        *state &= 0x7fff;
        u32::from(*state)
    } as i32;
    if lower > upper {
        return Err(RandomDomainError);
    }
    if lower == upper {
        return Ok(upper);
    }
    // Signed width is significant for the full legal span: 32768/2^31
    // becomes the negative minimum divisor while the dividend stays positive.
    let narrow = |v: i32| if wide { v } else { i32::from(v as i16) };
    let span = narrow(upper.wrapping_sub(lower).wrapping_add(1));
    let mut candidate = narrow(lower.wrapping_add(seed % span));
    let reflected = |v: i32, plus_one: bool| {
        narrow(
            upper
                .wrapping_sub(v)
                .wrapping_add(lower)
                .wrapping_add(i32::from(plus_one)),
        )
    };
    candidate = match (upper & 1 != 0, lower & 1 != 0) {
        (true, true) | (false, false) => {
            if candidate & 1 != 0 {
                candidate
            } else {
                reflected(candidate, false)
            }
        }
        (true, false) => {
            candidate = narrow(candidate.wrapping_add(1));
            if candidate & 1 != 0 {
                candidate
            } else {
                reflected(candidate, true)
            }
        }
        (false, true) => {
            candidate = narrow(candidate.wrapping_add(1));
            if candidate & 1 != 0 {
                reflected(candidate, true)
            } else {
                candidate
            }
        }
    };
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_update_width_and_error_order() {
        let mut state = 0xffff;
        assert_eq!(
            sample_with_state(-1, 10, false, &mut state),
            Err(RandomDomainError)
        );
        assert_eq!(state, 0xffff);
        assert_eq!(
            sample_with_state(10, 1, false, &mut state),
            Err(RandomDomainError)
        );
        assert_eq!(state, 99);
        assert_eq!(sample_with_state(7, 7, false, &mut state), Ok(7));
        assert_eq!(state, 199);
        let mut wide_state = 0xfffd;
        assert_eq!(sample_with_state(7, 7, true, &mut wide_state), Ok(7));
        assert_eq!(wide_state, 297);
        let mut word_state = 0x8000;
        sample_with_state(7, 7, false, &mut word_state).unwrap();
        assert_eq!(word_state, 0);
        let mut wide_state = 0x8000;
        sample_with_state(7, 7, true, &mut wide_state).unwrap();
        assert_eq!(wide_state, 0x8000);
    }

    #[test]
    fn recovered_body_remains_within_inclusive_bounds() {
        for wide in [false, true] {
            for (lower, upper) in [
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 4),
                (1, 9),
                (2, 10),
                (0, 32767),
                (32766, 32767),
            ] {
                for initial in [0, 1, 2, 3, 0x7fff, 0x8000, 0xffff] {
                    let mut state = initial;
                    for _ in 0..64 {
                        let sample = sample_with_state(lower, upper, wide, &mut state).unwrap();
                        assert!(
                            (lower..=upper).contains(&sample),
                            "{wide} {lower} {upper} {initial}: {sample}"
                        );
                    }
                }
            }
        }
        for initial in [0, 1, 2, 3, 0x7fff, 0x8000, 0xffff] {
            let mut state = initial;
            for _ in 0..64 {
                let sample = sample_with_state(0, i32::MAX, true, &mut state).unwrap();
                assert!(sample >= 0);
            }
        }
    }

    #[test]
    fn snapshots_aliased_bounds_holds_error_destination_and_flags() {
        let mut m = Memory::default();
        let source: Device = "D0".parse().unwrap();
        m.set_word(source, 9);
        for bit in 1020..=1022 {
            m.set_system(bit, true);
        }
        let equal = RandomAction::parse("RAND", &["D0".into(), "D0".into(), "D0".into()]).unwrap();
        equal.execute(&mut m);
        assert_eq!(m.word(source), 9);
        assert_eq!(m.random_state, Some(301));
        let reversed =
            RandomAction::parse("RAND", &["K10".into(), "K1".into(), "D0".into()]).unwrap();
        reversed.execute(&mut m);
        assert_eq!(m.word(source), 9);
        assert_eq!(m.random_state, Some(601));
        assert!(m.system_bit(1067) && m.system_bit(1068));
        assert_eq!(m.word("D1067".parse().unwrap()), 0x0e1a);
        assert_eq!(m.word("D1068".parse().unwrap()), 3);
        for bit in 1020..=1022 {
            assert!(m.system_bit(bit));
        }
    }
}
