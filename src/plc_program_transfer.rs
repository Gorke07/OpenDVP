//! Program-transfer PDU helpers for the owned SS2 simulator.
//!
//! The installed simulator has been observed using these 0x74/0x75 layouts.
//! Delta editor also contains other physical-device transfer paths: neither this
//! module nor a successful simulator test validates them for a physical PLC.
//! A PDU here starts with the function byte, and excludes station, LRC/CRC,
//! and ASCII/RTU framing. No device is opened or modified by these helpers.

const READ: u8 = 0x74;
const WRITE: u8 = 0x75;
const BANK_WRITE: u8 = 0x10;
const MAX_WORDS: usize = 100;
const SS2_BANK_WORDS: usize = 0x1ef0;
const SS2_SERIAL_BLOCK_WORDS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferError {
    InvalidRange,
    UnexpectedResponse,
    Exception(u8),
}

fn validate_range(start: u16, count: usize) -> Result<(), TransferError> {
    if !(1..=MAX_WORDS).contains(&count) || usize::from(start) + count > 65_536 {
        return Err(TransferError::InvalidRange);
    }
    Ok(())
}

/// Build the simulator's read request. The endpoint's actual capacity is not
/// established by this wire-level range check.
pub fn simulator_read_request(start: u16, count: usize) -> Result<Vec<u8>, TransferError> {
    validate_range(start, count)?;
    let mut pdu = vec![READ];
    pdu.extend_from_slice(&start.to_be_bytes());
    pdu.extend_from_slice(&(count as u16).to_be_bytes());
    Ok(pdu)
}

/// Build the simulator's write request. This does not perform a write.
pub fn simulator_write_request(start: u16, words: &[u16]) -> Result<Vec<u8>, TransferError> {
    validate_range(start, words.len())?;
    let mut pdu = vec![WRITE];
    pdu.extend_from_slice(&start.to_be_bytes());
    pdu.extend_from_slice(&(words.len() as u16).to_be_bytes());
    pdu.push((words.len() * 2) as u8);
    for word in words {
        pdu.extend_from_slice(&word.to_be_bytes());
    }
    Ok(pdu)
}

fn check_function(response: &[u8], expected: u8) -> Result<(), TransferError> {
    if response.first() == Some(&(expected | 0x80)) {
        return if response.len() == 2 {
            Err(TransferError::Exception(response[1]))
        } else {
            Err(TransferError::UnexpectedResponse)
        };
    }
    if response.first() != Some(&expected) {
        return Err(TransferError::UnexpectedResponse);
    }
    Ok(())
}

/// Validate a complete simulator read response and return its exact words.
pub fn simulator_read_response(response: &[u8], count: usize) -> Result<Vec<u16>, TransferError> {
    validate_range(0, count)?;
    check_function(response, READ)?;
    if response.len() != 2 + count * 2 || response[1] != (count * 2) as u8 {
        return Err(TransferError::UnexpectedResponse);
    }
    Ok(response[2..]
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .collect())
}

/// Validate the address/count echo observed from the owned simulator only.
/// No claim is made about the physical PLC acknowledgement format.
pub fn simulator_write_response(
    response: &[u8],
    start: u16,
    count: usize,
) -> Result<(), TransferError> {
    validate_range(start, count)?;
    check_function(response, WRITE)?;
    let mut expected = vec![WRITE];
    expected.extend_from_slice(&start.to_be_bytes());
    expected.extend_from_slice(&(count as u16).to_be_bytes());
    if response != expected {
        return Err(TransferError::UnexpectedResponse);
    }
    Ok(())
}

/// SS2 simulator's first program bank, using Delta editor's observed serial block
/// size. The mapping and limit are established for the simulator only.
pub fn ss2_simulator_bank_write_request(
    word_index: usize,
    words: &[u16],
) -> Result<Vec<u8>, TransferError> {
    if words.is_empty()
        || words.len() > SS2_SERIAL_BLOCK_WORDS
        || word_index >= SS2_BANK_WORDS
        || words.len() > SS2_BANK_WORDS - word_index
    {
        return Err(TransferError::InvalidRange);
    }
    let address = 0x2000u16 + word_index as u16;
    let mut pdu = vec![BANK_WRITE];
    pdu.extend_from_slice(&address.to_be_bytes());
    pdu.extend_from_slice(&(words.len() as u16).to_be_bytes());
    pdu.push((words.len() * 2) as u8);
    for word in words {
        pdu.extend_from_slice(&word.to_be_bytes());
    }
    Ok(pdu)
}

/// Validate the FC10 address/count echo observed from the owned simulator.
pub fn ss2_simulator_bank_write_response(
    response: &[u8],
    word_index: usize,
    count: usize,
) -> Result<(), TransferError> {
    if count == 0
        || count > SS2_SERIAL_BLOCK_WORDS
        || word_index >= SS2_BANK_WORDS
        || count > SS2_BANK_WORDS - word_index
    {
        return Err(TransferError::InvalidRange);
    }
    check_function(response, BANK_WRITE)?;
    let mut expected = vec![BANK_WRITE];
    expected.extend_from_slice(&(0x2000u16 + word_index as u16).to_be_bytes());
    expected.extend_from_slice(&(count as u16).to_be_bytes());
    if response != expected {
        return Err(TransferError::UnexpectedResponse);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulator_vectors_use_big_endian_words_and_exact_echo() {
        assert_eq!(
            simulator_read_request(0x1234, 2).unwrap(),
            [0x74, 0x12, 0x34, 0, 2]
        );
        assert_eq!(
            simulator_write_request(0x1234, &[0xabcd, 0x0123]).unwrap(),
            [0x75, 0x12, 0x34, 0, 2, 4, 0xab, 0xcd, 1, 0x23]
        );
        assert_eq!(
            simulator_read_response(&[0x74, 4, 0xab, 0xcd, 1, 0x23], 2).unwrap(),
            [0xabcd, 0x0123]
        );
        assert_eq!(
            simulator_write_response(&[0x75, 0x12, 0x34, 0, 2], 0x1234, 2),
            Ok(())
        );
    }

    #[test]
    fn rejects_invalid_or_overflowing_ranges() {
        assert_eq!(
            simulator_read_request(0, 0),
            Err(TransferError::InvalidRange)
        );
        assert_eq!(
            simulator_read_request(0, 101),
            Err(TransferError::InvalidRange)
        );
        assert_eq!(
            simulator_write_request(0xffff, &[1, 2]),
            Err(TransferError::InvalidRange)
        );
        assert!(simulator_read_request(0xffff, 1).is_ok());
        let request = simulator_write_request(0, &[0x1234; 100]).unwrap();
        assert_eq!(request.len(), 206);
        assert_eq!(request[5], 200);
    }

    #[test]
    fn rejects_wrong_function_length_count_and_echo() {
        for response in [
            &[0x73, 2, 0, 1][..],
            &[0x74, 0, 0, 1],
            &[0x74, 2, 0],
            &[0x74, 2, 0, 1, 2],
        ] {
            assert_eq!(
                simulator_read_response(response, 1),
                Err(TransferError::UnexpectedResponse)
            );
        }
        assert_eq!(
            simulator_write_response(&[0x75, 0, 0, 0, 2], 0, 1),
            Err(TransferError::UnexpectedResponse)
        );
        assert_eq!(
            simulator_write_response(&[0x75, 0, 1, 0, 1], 0, 1),
            Err(TransferError::UnexpectedResponse)
        );
        assert_eq!(
            simulator_write_response(&[0x75, 0, 0, 0, 1, 0], 0, 1),
            Err(TransferError::UnexpectedResponse)
        );
    }

    #[test]
    fn handles_well_formed_exception_and_rejects_malformed_exception() {
        assert_eq!(
            simulator_read_response(&[0xf4, 2], 1),
            Err(TransferError::Exception(2))
        );
        assert_eq!(
            simulator_write_response(&[0xf5, 3], 0, 1),
            Err(TransferError::Exception(3))
        );
        assert_eq!(
            simulator_read_response(&[0xf4], 1),
            Err(TransferError::UnexpectedResponse)
        );
        assert_eq!(
            simulator_write_response(&[0xf5, 3, 0], 0, 1),
            Err(TransferError::UnexpectedResponse)
        );
    }

    #[test]
    fn ss2_simulator_bank_vectors_and_bounds() {
        assert_eq!(
            ss2_simulator_bank_write_request(1, &[0xa55a]).unwrap(),
            [0x10, 0x20, 0x01, 0, 1, 2, 0xa5, 0x5a]
        );
        assert_eq!(
            ss2_simulator_bank_write_response(&[0x10, 0x20, 0x01, 0, 1], 1, 1),
            Ok(())
        );
        assert_eq!(
            ss2_simulator_bank_write_request(0x1eef, &[0xa55a]).unwrap(),
            [0x10, 0x3e, 0xef, 0, 1, 2, 0xa5, 0x5a]
        );
        assert!(ss2_simulator_bank_write_request(0x1ef0, &[0xa55a]).is_err());
        assert!(ss2_simulator_bank_write_request(0x1eef, &[0, 0]).is_err());
        assert!(ss2_simulator_bank_write_request(0, &[0; 17]).is_err());
        assert_eq!(
            ss2_simulator_bank_write_response(&[0x90, 3], 0, 1),
            Err(TransferError::Exception(3))
        );
        assert_eq!(
            ss2_simulator_bank_write_response(&[0x10, 0x20, 0x02, 0, 1], 1, 1),
            Err(TransferError::UnexpectedResponse)
        );
    }
}
