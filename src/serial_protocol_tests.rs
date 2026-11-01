use super::*;

fn frame(function: u8, data: &[u8]) -> Frame {
    Frame {
        station: 1,
        function,
        data: data.to_vec(),
    }
}

fn exchange(peer: &mut VirtualModbusPeer, request: &Frame) -> Frame {
    let wire = encode(WireMode::Rtu, request).unwrap();
    decode(
        WireMode::Rtu,
        &peer.respond(WireMode::Rtu, &wire).unwrap().unwrap(),
    )
    .unwrap()
}

#[test]
fn independently_published_delta_ascii_packets() {
    // Module manual PDF pages 663/664/666. Reply table LRC F8 is correct;
    // nearby prose F6 is contradictory and intentionally not our expected value.
    let pairs = [
        (frame(3, &[0, 0, 0, 1]), b":010300000001FB\r\n".as_slice()),
        (frame(3, &[2, 2, 0]), b":0103020200F8\r\n".as_slice()),
    ];
    for (f, wire) in pairs {
        assert_eq!(decode(WireMode::Ascii, wire).unwrap(), f);
        assert_eq!(encode(WireMode::Ascii, &f).unwrap(), wire);
    }
    assert!(decode(WireMode::Ascii, b":0103020200F6\r\n").is_err());
}

#[test]
fn independently_published_delta_rtu_packets() {
    let pairs: [(Frame, &[u8]); 4] = [
        (frame(3, &[0, 0, 0, 1]), &[1, 3, 0, 0, 0, 1, 0x84, 0x0a]),
        (frame(3, &[2, 2, 0]), &[1, 3, 2, 2, 0, 0xb9, 0x24]),
        (frame(3, &[0x16, 0, 0, 3]), &[1, 3, 0x16, 0, 0, 3, 1, 0x83]),
        (
            frame(3, &[6, 0, 1, 0, 2, 0, 3]),
            &[1, 3, 6, 0, 1, 0, 2, 0, 3, 0xfd, 0x74],
        ),
    ];
    for (f, wire) in pairs {
        assert_eq!(decode(WireMode::Rtu, wire).unwrap(), f);
        assert_eq!(encode(WireMode::Rtu, &f).unwrap(), wire);
    }
}

#[test]
fn bad_delimiters_digits_truncation_checksum_and_concatenation_are_rejected() {
    for invalid in [
        b"".as_slice(),
        b":010300000001FB\n",
        b":010300000001fb\r\n",
        b":010300000001F\r\n",
        b":010300000001FC\r\n",
        b":01G300000001FB\r\n",
        b" :010300000001FB\r\n",
        b":010300000001FB\r\n:010300000001FB\r\n",
    ] {
        assert!(decode(WireMode::Ascii, invalid).is_err(), "{invalid:?}");
    }
    for invalid in [
        b"".as_slice(),
        &[1, 3, 0],
        &[1, 3, 0, 0, 0, 1, 0x84, 0x0b],
        &[1, 3, 0, 0, 0, 1, 0x0a, 0x84],
    ] {
        assert!(decode(WireMode::Rtu, invalid).is_err());
    }
}

#[test]
fn swapped_crc_is_explicit_and_framing_does_not_apply_cpu_station_limits() {
    let f = frame(3, &[0, 0, 0, 1]);
    let bytes = [1, 3, 0, 0, 0, 1, 0x0a, 0x84];
    assert_eq!(encode(WireMode::RtuSwappedCrc, &f).unwrap(), bytes);
    assert_eq!(decode(WireMode::RtuSwappedCrc, &bytes).unwrap(), f);
    let reserved = Frame { station: 255, ..f };
    assert_eq!(
        decode(
            WireMode::Ascii,
            &encode(WireMode::Ascii, &reserved).unwrap()
        )
        .unwrap(),
        reserved
    );
    assert!(VirtualModbusPeer::new(255).is_err());
    assert!(VirtualModbusPeer::new(0).is_err());
}

#[test]
fn frame_size_limits_include_station_function_and_checksum() {
    for (mode, max) in [
        (WireMode::Ascii, 513),
        (WireMode::Rtu, 256),
        (WireMode::RtuSwappedCrc, 256),
    ] {
        let f = frame(3, &[0; 252]);
        let bytes = encode(mode, &f).unwrap();
        assert_eq!(bytes.len(), max);
        assert_eq!(decode(mode, &bytes).unwrap(), f);
        assert!(encode(mode, &frame(3, &[0; 253])).is_err());
        assert!(decode(mode, &[0; 514]).is_err());
    }
}

#[test]
fn response_identity_lengths_and_ack_are_exact() {
    let req = frame(3, &[0, 0, 0, 1]);
    let good = frame(3, &[2, 0x12, 0x34]);
    assert_eq!(
        validate_response(&req, &good).unwrap(),
        Reply::Registers(vec![0x1234])
    );
    for bad in [
        Frame {
            station: 2,
            ..good.clone()
        },
        frame(4, &[2, 0x12, 0x34]),
        frame(3, &[1, 0x12, 0x34]),
        frame(3, &[2, 0x12]),
        frame(3, &[]),
    ] {
        assert!(validate_response(&req, &bad).is_err());
    }
    let req = frame(16, &[0, 1, 0, 2, 4, 0, 10, 1, 2]);
    assert_eq!(
        validate_response(&req, &frame(16, &[0, 1, 0, 2])).unwrap(),
        Reply::WriteAck
    );
    assert!(validate_response(&req, &frame(16, &[0, 1, 0, 1])).is_err());
    assert!(validate_response(&req, &req).is_err());
    for f in [5, 6] {
        let req = frame(f, &[0, 1, 0, 0]);
        assert_eq!(validate_response(&req, &req).unwrap(), Reply::WriteAck);
        assert!(validate_response(&req, &frame(f, &[0, 2, 0, 0])).is_err());
    }
}

#[test]
fn exceptions_are_preserved_separately_from_success() {
    let req = frame(3, &[0, 0, 0, 1]);
    assert_eq!(
        validate_response(&req, &frame(0x83, &[2])).unwrap(),
        Reply::Exception(2)
    );
    for bad in [
        frame(0x84, &[2]),
        frame(0x83, &[]),
        frame(0x83, &[0]),
        frame(0x83, &[2, 0]),
    ] {
        assert!(validate_response(&req, &bad).is_err());
    }
    assert!(
        validate_response(
            &Frame {
                station: 0,
                ..req.clone()
            },
            &frame(0x83, &[2])
        )
        .is_err()
    );
}

#[test]
fn bit_responses_are_lsb_first_and_strict_about_padding() {
    let req = frame(1, &[0, 19, 0, 10]);
    assert_eq!(
        validate_response(&req, &frame(1, &[2, 0xcd, 1])).unwrap(),
        Reply::Bits(vec![
            true, false, true, true, false, false, true, true, true, false
        ])
    );
    for invalid in [
        frame(1, &[]),
        frame(1, &[0]),
        frame(1, &[1, 0xcd]),
        frame(1, &[2, 0xcd, 0x81]),
    ] {
        assert!(validate_response(&req, &invalid).is_err());
    }
}

#[test]
fn sparse_peer_has_four_distinct_address_spaces() {
    let mut p = VirtualModbusPeer::new(1).unwrap();
    p.coils.insert(10, true);
    p.discrete_inputs.insert(10, false);
    p.holding.insert(10, 0x1234);
    p.input_registers.insert(10, 0xabcd);
    for (function, bytes) in [
        (1, vec![1, 1]),
        (2, vec![1, 0]),
        (3, vec![2, 0x12, 0x34]),
        (4, vec![2, 0xab, 0xcd]),
    ] {
        assert_eq!(
            exchange(&mut p, &frame(function, &[0, 10, 0, 1])).data,
            bytes
        );
        assert_eq!(
            exchange(&mut p, &frame(function, &[0, 11, 0, 1])),
            frame(function | 0x80, &[2])
        );
    }
}

#[test]
fn official_multiple_write_examples_and_atomic_address_failure() {
    let mut p = VirtualModbusPeer::new(1).unwrap();
    for a in 19..29 {
        p.coils.insert(a, false);
    }
    for a in 1..3 {
        p.holding.insert(a, 0);
    }
    // Modbus Application Protocol V1.1b3 FC0F/FC10 examples.
    let coil_request = frame(15, &[0, 19, 0, 10, 2, 0xcd, 1]);
    assert_eq!(exchange(&mut p, &coil_request), frame(15, &[0, 19, 0, 10]));
    assert_eq!(
        p.coils.values().copied().collect::<Vec<_>>(),
        vec![
            true, false, true, true, false, false, true, true, true, false
        ]
    );
    let words = frame(16, &[0, 1, 0, 2, 4, 0, 10, 1, 2]);
    assert_eq!(exchange(&mut p, &words), frame(16, &[0, 1, 0, 2]));
    assert_eq!(p.holding[&1], 10);
    assert_eq!(p.holding[&2], 258);
    let before = p.clone();
    assert_eq!(
        exchange(&mut p, &frame(16, &[0, 2, 0, 2, 4, 0, 20, 0, 30])),
        frame(0x90, &[2])
    );
    assert_eq!(p, before);
    assert_eq!(
        exchange(&mut p, &frame(15, &[0, 28, 0, 2, 1, 3])),
        frame(0x8f, &[2])
    );
    assert_eq!(p, before);
}

#[test]
fn read_write_preflights_both_ranges_then_reads_after_write() {
    let mut p = VirtualModbusPeer::new(1).unwrap();
    p.holding.extend([(10, 1), (11, 2), (12, 3)]);
    let req = frame(23, &[0, 10, 0, 3, 0, 11, 0, 2, 4, 0, 99, 0, 88]);
    let reply = exchange(&mut p, &req);
    assert_eq!(reply, frame(23, &[6, 0, 1, 0, 99, 0, 88]));
    assert_eq!(
        validate_response(&req, &reply).unwrap(),
        Reply::Registers(vec![1, 99, 88])
    );
    let before = p.clone();
    for data in [
        vec![0, 9, 0, 3, 0, 11, 0, 2, 4, 0, 9, 0, 8],
        vec![0, 10, 0, 3, 0, 12, 0, 2, 4, 0, 9, 0, 8],
    ] {
        assert_eq!(exchange(&mut p, &frame(23, &data)), frame(0x97, &[2]));
        assert_eq!(p, before);
    }
}

#[test]
fn broadcast_executes_writes_without_reply_and_ignores_reads_and_other_stations() {
    let mut p = VirtualModbusPeer::new(1).unwrap();
    p.holding.insert(0, 1);
    let req = Frame {
        station: 0,
        ..frame(6, &[0, 0, 0, 9])
    };
    assert_eq!(
        p.respond(WireMode::Ascii, &encode(WireMode::Ascii, &req).unwrap())
            .unwrap(),
        None
    );
    assert_eq!(p.holding[&0], 9);
    for req in [
        Frame {
            station: 2,
            ..frame(6, &[0, 0, 0, 7])
        },
        Frame {
            station: 0,
            ..frame(3, &[0, 0, 0, 1])
        },
        Frame {
            station: 0,
            ..frame(23, &[0, 0, 0, 1, 0, 0, 0, 1, 2, 0, 8])
        },
    ] {
        assert_eq!(
            p.respond(WireMode::Rtu, &encode(WireMode::Rtu, &req).unwrap())
                .unwrap(),
            None
        );
        assert_eq!(p.holding[&0], 9);
    }
}

#[test]
fn malformed_frame_and_request_never_mutate_peer() {
    let mut p = VirtualModbusPeer::new(1).unwrap();
    p.holding.insert(0, 3);
    p.coils.insert(0, false);
    let before = p.clone();
    assert!(p.respond(WireMode::Rtu, &[1, 6, 0, 0, 0, 9, 0, 0]).is_err());
    for req in [
        frame(5, &[0, 0, 0, 1]),
        frame(6, &[0, 0, 1]),
        frame(15, &[0, 0, 0, 1, 1, 0x81]),
        frame(16, &[0, 0, 0, 1, 1, 3]),
        frame(23, &[0, 0, 0, 1, 0, 0, 0, 1, 1, 3]),
    ] {
        assert_eq!(exchange(&mut p, &req), frame(req.function | 0x80, &[3]));
        assert_eq!(p, before);
    }
    assert_eq!(exchange(&mut p, &frame(65, &[])), frame(0xc1, &[1]));
    assert_eq!(p, before);
}

#[test]
fn quantities_and_address_spans_are_bounded_without_wrapping() {
    for (f, limit) in [(1, 2000u16), (2, 2000), (3, 125), (4, 125)] {
        for n in [0, limit + 1] {
            assert_eq!(
                request(&frame(f, &[0, 0, (n >> 8) as u8, n as u8])).err(),
                Some(3)
            );
        }
        assert!(request(&frame(f, &[0, 0, (limit >> 8) as u8, limit as u8])).is_ok());
        assert!(request(&frame(f, &[255, 255, 0, 1])).is_ok());
        assert_eq!(request(&frame(f, &[255, 255, 0, 2])).err(), Some(2));
    }
    for (f, n) in [(15, 1968u16), (16, 123)] {
        let bytes = if f == 15 {
            usize::from(n).div_ceil(8)
        } else {
            usize::from(n) * 2
        };
        let mut data = vec![0, 0, (n >> 8) as u8, n as u8, bytes as u8];
        data.resize(5 + bytes, 0);
        assert!(request(&frame(f, &data)).is_ok());
        data[2] = ((n + 1) >> 8) as u8;
        data[3] = (n + 1) as u8;
        assert_eq!(request(&frame(f, &data)).err(), Some(3));
    }
    let mut data = vec![0, 0, 0, 125, 0, 0, 0, 121, 242];
    data.resize(251, 0);
    assert!(request(&frame(23, &data)).is_ok());
    data[7] = 122;
    assert_eq!(request(&frame(23, &data)).err(), Some(3));
}
