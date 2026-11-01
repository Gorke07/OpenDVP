//! Four original time-arithmetic descriptor WORDs, entered only on success.
use crate::plc::{Action, Program};
use crate::random_tail::OperandTailControl;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CalendarTailProfile {
    /// Explicit standalone policy; nominal seven-WORD arithmetic visits.
    #[default]
    Documented,
    /// Only the exact current COMMGR images with authenticated dispatchers.
    CurrentCommgr,
    /// Other native families remain available; time-tail parity is unqualified.
    UnqualifiedNative,
}

pub fn decode_time_tail(words: &[u16]) -> Result<[Option<OperandTailControl>; 4], String> {
    if words.len() != 7 || !matches!(words[0], 0x6401 | 0x6411 | 0x6601 | 0x6611) {
        return Err("Time arithmetic tail requires its original seven WORDs".into());
    }
    let mut decoded = [None; 4];
    for (slot, word) in decoded.iter_mut().zip(&words[3..]) {
        *slot = match word & 0xf0 {
            0x80 | 0x90 | 0xe0 => None,
            0xf0 => match word >> 8 {
                0xf6 => Some(OperandTailControl::Np),
                0xf8 => Some(OperandTailControl::AndBlock),
                0xfa => Some(OperandTailControl::Mps),
                0xfc => Some(OperandTailControl::Mpp),
                // Only aligned T/C/D byte offsets are legal in these roles.
                0xf7 | 0xf9 | 0xfb | 0xfd => {
                    return Err("Unaligned time-arithmetic control descriptor".into());
                }
                _ => None,
            },
            _ => return Err(format!("Unqualified time operand-tail WORD H{word:04X}")),
        };
    }
    Ok(decoded)
}

pub(crate) fn is_arithmetic(action: &Action) -> bool {
    match action {
        Action::Pulse(inner) => is_arithmetic(inner),
        Action::Time(crate::time_ops::TimeAction::Arithmetic { .. }) => true,
        Action::IndexedCalendarTime(a) => a.is_arithmetic(),
        _ => false,
    }
}
pub(crate) fn instruction_words(action: &Action) -> Result<Option<Vec<u16>>, String> {
    match action {
        Action::Pulse(inner) => {
            let mut words = instruction_words(inner)?;
            if let Some(words) = &mut words {
                words[0] |= 0x10;
            }
            Ok(words)
        }
        Action::Time(a @ crate::time_ops::TimeAction::Arithmetic { .. }) => {
            a.native_words().map(Some)
        }
        Action::IndexedCalendarTime(a) if a.is_arithmetic() => a.native_words().map(Some),
        _ => Ok(None),
    }
}

pub(crate) fn qualify_outcomes(
    local: &crate::time_ops::CalendarTimeOutcome,
    native: &crate::time_ops::CalendarTimeOutcome,
) -> Result<(), String> {
    use crate::time_ops::CalendarTimeOutcome;
    if let CalendarTimeOutcome::Limitation(reason) = native {
        return Err(format!("Native calendar operands unqualified: {reason}"));
    }
    if let CalendarTimeOutcome::Limitation(reason) = local {
        return Err(format!(
            "Independent local calendar operands unqualified: {reason}"
        ));
    }
    if local != native {
        return Err(format!(
            "Inconclusive calendar visit: local {local:?}, native-input replay {native:?}; no native STEP qualified"
        ));
    }
    Ok(())
}
pub(crate) fn validate_program_profile(
    program: &Program,
    profile: CalendarTailProfile,
) -> Result<(), String> {
    for action in program.networks.iter().flat_map(|n| &n.actions) {
        let inner = if let Action::Pulse(inner) = action {
            inner.as_ref()
        } else {
            action
        };
        if profile == CalendarTailProfile::UnqualifiedNative
            && (is_arithmetic(action) || matches!(inner, Action::IndexedCalendarTime(_)))
        {
            return Err(
                "Calendar operands/header gate are unqualified for this native image".into(),
            );
        }
        if let Some(words) = instruction_words(action)? {
            decode_time_tail(&words)?;
        }
    }
    Ok(())
}
