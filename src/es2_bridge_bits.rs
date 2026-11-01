//! Read-only bit batching for the parent's owned, paused simulator session.
//! Blocks never bridge an area or a discontinuity in its protocol address map.
use super::{Session, bit_address};
use crate::plc::{Area, Device};
use std::collections::{BTreeMap, BTreeSet};

const MAX_BITS: usize = 128;
/// Unused bits a block may span to save a request. Every request costs a fixed
/// turnaround (about 15.6 ms on the simulators, more on a serial PLC), so reading a
/// few unused bits is cheaper than a second request.
const MAX_BIT_GAP: u16 = 32;

/// Canonicalize unordered/duplicate input explicitly, then partition by protocol
/// address. A block may skip unused bits of the same area, but only where the wire
/// address map is continuous: logical M1535/M1536 are not adjacent on this wire.
pub(super) fn bit_read_blocks(devices: &[Device]) -> Result<Vec<Vec<Device>>, String> {
    let devices = devices.iter().copied().collect::<BTreeSet<_>>();
    let mut blocks: Vec<Vec<Device>> = Vec::new();
    for device in devices {
        let address = bit_address(device)?;
        if let Some(block) = blocks.last_mut()
            && block[0].area == device.area
        {
            let first = bit_address(block[0])?;
            let last = *block.last().unwrap();
            let step = address.checked_sub(bit_address(last)?);
            let continuous = step == device.index.checked_sub(last.index);
            if continuous
                && step.is_some_and(|s| s <= MAX_BIT_GAP)
                && usize::from(address - first) < MAX_BITS
            {
                block.push(device);
                continue;
            }
        }
        blocks.push(vec![device]);
    }
    Ok(blocks)
}

/// Number of bits a block reads: from its first to its last device.
fn block_span(block: &[Device]) -> Result<usize, String> {
    let first = bit_address(block[0])?;
    let last = bit_address(*block.last().ok_or("Empty bit block")?)?;
    Ok(usize::from(last - first) + 1)
}

fn decode_bit_block(response: &[u8], count: usize) -> Result<Vec<bool>, String> {
    if !(1..=MAX_BITS).contains(&count) {
        return Err("Bit read count must be 1..128".into());
    }
    let bytes = count.div_ceil(8);
    if response.len() != bytes + 1 || usize::from(response[0]) != bytes {
        return Err(format!(
            "ES2 bit-read byte count mismatch: expected {bytes} payload bytes for {count} bits"
        ));
    }
    // Modbus coils are LSB first. Ignore only unused bits in the final byte;
    // reject truncated, extended or inconsistent byte-count responses above.
    Ok((0..count)
        .map(|bit| response[1 + bit / 8] & (1 << (bit & 7)) != 0)
        .collect())
}

impl Session {
    pub(super) fn read_bit_devices(
        &mut self,
        devices: &[Device],
    ) -> Result<BTreeMap<Device, bool>, String> {
        let mut result = BTreeMap::new();
        for block in bit_read_blocks(devices)? {
            let address = bit_address(block[0])?;
            let span = block_span(&block)?;
            let mut request = address.to_be_bytes().to_vec();
            request.extend_from_slice(&(span as u16).to_be_bytes());
            let function = if block[0].area == Area::X { 2 } else { 1 };
            let response = self.request(function, &request)?;
            let values = decode_bit_block(&response, span)?;
            for device in block {
                let offset = usize::from(bit_address(device)? - address);
                result.insert(device, values[offset]);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(area: Area, index: u16) -> Device {
        Device { area, index }
    }

    #[test]
    fn full_s_image_is_exactly_eight_maximum_size_blocks() {
        let devices = (0..1024).map(|n| d(Area::S, n)).collect::<Vec<_>>();
        let blocks = bit_read_blocks(&devices).unwrap();
        assert_eq!(blocks.len(), 8);
        assert!(blocks.iter().all(|b| b.len() == 128));
        assert_eq!(blocks.into_iter().flatten().collect::<Vec<_>>(), devices);
    }
    #[test]
    fn counts_gaps_and_area_boundaries_remain_distinct() {
        for count in [1, 7, 8, 9, 127, 128, 129, 255, 256] {
            let devices = (0..count).map(|n| d(Area::S, n)).collect::<Vec<_>>();
            let blocks = bit_read_blocks(&devices).unwrap();
            assert_eq!(blocks.len(), usize::from(count).div_ceil(128));
            assert!(blocks.iter().all(|b| b.len() <= 128));
            assert_eq!(blocks.into_iter().flatten().collect::<Vec<_>>(), devices);
        }
        let blocks = bit_read_blocks(&[
            d(Area::S, 1023),
            d(Area::X, 0),
            d(Area::X, 255),
            d(Area::Y, 0),
            d(Area::M, 1535),
            d(Area::M, 1536),
            d(Area::M, 1537),
        ])
        .unwrap();
        assert_eq!(blocks.len(), 6);
        assert_eq!(
            blocks.iter().map(Vec::len).collect::<Vec<_>>(),
            [1, 1, 1, 1, 2, 1]
        );
        // Device sort order is X,Y,M,S; area equality must be checked even when
        // two areas' protocol end/start happen to be numerically contiguous.
        assert!(blocks.iter().all(|b| b.iter().all(|v| v.area == b[0].area)));
        assert!(
            blocks
                .iter()
                .all(|b| !(b.contains(&d(Area::M, 1535)) && b.contains(&d(Area::M, 1536))))
        );
    }
    #[test]
    fn small_gaps_share_one_request_within_the_wire_map() {
        // X0..X3, X10 (octal: protocol address +8) and X20 fit one read.
        let blocks =
            bit_read_blocks(&[d(Area::X, 0), d(Area::X, 3), d(Area::X, 8), d(Area::X, 16)])
                .unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(block_span(&blocks[0]).unwrap(), 17);
        // A gap larger than MAX_BIT_GAP starts a new request.
        let blocks = bit_read_blocks(&[d(Area::M, 0), d(Area::M, 100)]).unwrap();
        assert_eq!(blocks.len(), 2);
        // The span never exceeds one 128-bit read.
        let spread = (0..8).map(|i| d(Area::S, i * 30)).collect::<Vec<_>>();
        for block in bit_read_blocks(&spread).unwrap() {
            assert!(block_span(&block).unwrap() <= MAX_BITS);
        }
    }
    #[test]
    fn unordered_duplicates_are_canonicalized_and_non_bit_or_invalid_rejected() {
        let input = [d(Area::S, 9), d(Area::S, 7), d(Area::S, 8), d(Area::S, 7)];
        assert_eq!(
            bit_read_blocks(&input).unwrap(),
            vec![vec![d(Area::S, 7), d(Area::S, 8), d(Area::S, 9)]]
        );
        assert!(bit_read_blocks(&[]).unwrap().is_empty());
        for device in [
            d(Area::D, 0),
            d(Area::S, 1024),
            d(Area::Y, 256),
            d(Area::M, 4096),
        ] {
            assert!(bit_read_blocks(&[device]).is_err());
        }
    }
    #[test]
    fn decoder_checks_exact_count_and_lsb_order_at_byte_boundaries() {
        assert_eq!(
            decode_bit_block(&[2, 0x81, 0xfe], 9).unwrap(),
            [true, false, false, false, false, false, false, true, false]
        );
        assert_eq!(decode_bit_block(&[1, 0xff], 7).unwrap(), vec![true; 7]);
        let mut response = vec![16];
        response.extend([0x81; 16]);
        let values = decode_bit_block(&response, 128).unwrap();
        for (index, value) in values.into_iter().enumerate() {
            assert_eq!(value, index % 8 == 0 || index % 8 == 7);
        }
        for (response, count) in [
            (vec![], 1),
            (vec![0], 1),
            (vec![1], 1),
            (vec![2, 1], 1),
            (vec![1, 1, 0], 1),
            (vec![1, 1, 2], 9),
            (vec![1, 1], 0),
            (vec![1, 1], 129),
        ] {
            assert!(decode_bit_block(&response, count).is_err());
        }
    }
    #[test]
    #[ignore = "requires installed digest-verified ES2/SS2; launches only Session-owned original processes"]
    fn native_batched_bits_match_single_reads_full_s_y_and_changed_x() {
        use super::super::{DEFAULT_SIMULATOR, DEFAULT_SS2_SIMULATOR};
        use std::path::Path;
        use std::sync::{Arc, atomic::AtomicBool};
        for path in [DEFAULT_SIMULATOR, DEFAULT_SS2_SIMULATOR] {
            let mut session =
                Session::launch(Path::new(path), Arc::new(AtomicBool::new(false))).unwrap();
            let steps = [0, 7, 127, 128, 895, 896, 1023];
            let outputs = [d(Area::Y, 0), d(Area::Y, 7), d(Area::Y, 255)];
            let relays = [
                d(Area::M, 300),
                d(Area::M, 1535),
                d(Area::M, 2000),
                d(Area::M, 4095),
            ];
            let mut source = String::from("LD X0\n");
            for step in steps {
                source.push_str(&format!("SET S{step}\n"));
            }
            for device in outputs.into_iter().chain(relays) {
                source.push_str(&format!("SET {device}\n"));
            }
            source.push_str("END");
            session.upload(&source).unwrap();
            let devices = (0..1024)
                .map(|n| d(Area::S, n))
                .chain((0..256).map(|n| d(Area::Y, n)))
                .chain((0..256).map(|n| d(Area::X, n)))
                .chain(relays)
                .chain([d(Area::M, 1536), d(Area::T, 0), d(Area::C, 255)])
                .collect::<Vec<_>>();
            for inputs in [
                vec![
                    (d(Area::X, 0), true),
                    (d(Area::X, 7), true),
                    (d(Area::X, 255), true),
                ],
                vec![(d(Area::X, 7), false), (d(Area::X, 128), true)],
            ] {
                assert_eq!(session.step(&inputs).unwrap().mismatches, 0);
                // Reads only after the owned target is paused. Neither reading
                // path patches results, copies native values locally, or writes
                // memory. Seed/input writes use ordinary Session program/step.
                let batched = session.read_bit_devices(&devices).unwrap();
                for device in &devices {
                    assert_eq!(
                        batched[device],
                        session.bit_read(*device).unwrap(),
                        "{} {device}",
                        session.target().name()
                    );
                }
                for step in steps {
                    assert!(batched[&d(Area::S, step)]);
                }
                for device in outputs.into_iter().chain(relays) {
                    assert!(batched[&device]);
                }
                for (device, value) in inputs {
                    assert_eq!(batched[&device], value);
                }
            }
            println!(
                "{}: full1024 S/256 Y/256 X and relay windows batch==single; owned paused target, no result writes",
                session.target().name()
            );
        }
    }
}
