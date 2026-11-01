//! Native SS2 observations only: the test owns a fresh verified simulator.
//! These observations do not establish behavior on a physical PLC.
use super::*;

#[test]
#[ignore = "requires installed digest-verified SS2 simulator"]
fn ss2_fc10_first_program_bank_matches_fc74_and_capacity_boundary() {
    let mut session = Session::launch(
        Path::new(DEFAULT_SS2_SIMULATOR),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();

    assert_eq!(session.sha256(), COMMGR_SS2_SHA256);
    assert!(session.set_teaching_tick_replay(Some(1)).is_err());
    assert!(session.set_teaching_tick_replay(None).is_ok());

    // D1002 reports 0x1ef0 on this simulator. This is an observed simulator
    // register value, not evidence of any particular physical PLC identity.
    assert_eq!(
        session.request(3, &[0x13, 0xea, 0, 1]).unwrap(),
        [2, 0x1e, 0xf0]
    );

    let block: Vec<u16> = (0..16).map(|i| 0x1200 + i).collect();
    let write = crate::plc_program_transfer::ss2_simulator_bank_write_request(0, &block).unwrap();
    let response = session.request(write[0], &write[1..]).unwrap();
    let mut pdu = vec![0x10];
    pdu.extend(response);
    crate::plc_program_transfer::ss2_simulator_bank_write_response(&pdu, 0, block.len()).unwrap();
    let response = session.request(0x74, &[0, 0, 0, 16]).unwrap();
    let mut pdu = vec![0x74];
    pdu.extend(response);
    assert_eq!(
        crate::plc_program_transfer::simulator_read_response(&pdu, 16).unwrap(),
        block
    );

    for index in [1u16, 0x0fff, 0x1000, 0x1eee, 0x1eef] {
        let write = crate::plc_program_transfer::ss2_simulator_bank_write_request(
            usize::from(index),
            &[0xa55a],
        )
        .unwrap();
        let response = session.request(write[0], &write[1..]).unwrap();
        let mut pdu = vec![0x10];
        pdu.extend(response);
        crate::plc_program_transfer::ss2_simulator_bank_write_response(&pdu, usize::from(index), 1)
            .unwrap();
        let [a, b] = index.to_be_bytes();
        assert_eq!(
            session.request(0x74, &[a, b, 0, 1]).unwrap(),
            [2, 0xa5, 0x5a]
        );
    }

    assert!(
        session
            .request(0x10, &[0x3e, 0xf0, 0, 1, 2, 0xa5, 0x5a])
            .is_err()
    );
    // Unlike FC10, this simulator replies to an out-of-range FC74 request
    // with function 0x74 and data ff 03; do not treat it as a data block.
    assert_eq!(
        session.request(0x74, &[0x1e, 0xf0, 0, 1]).unwrap(),
        [0xff, 3]
    );
    assert!(
        session
            .request(0x10, &[0x50, 0, 0, 1, 2, 0xa5, 0x5a])
            .is_err()
    );
}

#[test]
#[ignore = "requires installed digest-verified SS2 simulator"]
fn ss2_fc10_encoded_program_roundtrips_and_runs() {
    let mut session = Session::launch(
        Path::new(DEFAULT_SS2_SIMULATOR),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    let source = format!("LD X0\n{}OUT Y0\nEND\n", "MOV K123 D0\n".repeat(20));
    let program = plc::compile(&source).unwrap();
    let encoded = es2_encoder::encode(&program).unwrap();
    assert!(encoded.words.len() > 16);

    // These control bits are known only for this owned simulator. They are
    // not a verified physical PLC stop/commit/start sequence.
    session.bit_write(RUN, false).unwrap();
    for (chunk_index, words) in encoded.words.chunks(16).enumerate() {
        let start = chunk_index * 16;
        let request =
            crate::plc_program_transfer::ss2_simulator_bank_write_request(start, words).unwrap();
        let response = session.request(request[0], &request[1..]).unwrap();
        let pdu = std::iter::once(0x10).chain(response).collect::<Vec<_>>();
        crate::plc_program_transfer::ss2_simulator_bank_write_response(&pdu, start, words.len())
            .unwrap();
    }

    let mut readback = Vec::new();
    for (chunk_index, words) in encoded.words.chunks(16).enumerate() {
        let request = crate::plc_program_transfer::simulator_read_request(
            (chunk_index * 16) as u16,
            words.len(),
        )
        .unwrap();
        let response = session.request(request[0], &request[1..]).unwrap();
        let pdu = std::iter::once(0x74).chain(response).collect::<Vec<_>>();
        readback.extend(
            crate::plc_program_transfer::simulator_read_response(&pdu, words.len()).unwrap(),
        );
    }
    assert_eq!(readback, encoded.words);

    // Delta editor's observed alternate read uses FC03 in the 0x2000 bank.
    // This assertion establishes that path for this simulator only.
    let mut bank_readback = Vec::new();
    for (chunk_index, words) in encoded.words.chunks(16).enumerate() {
        bank_readback.extend(
            session
                .words_read(0x2000 + (chunk_index * 16) as u16, words.len() as u16)
                .unwrap(),
        );
    }
    assert_eq!(bank_readback, encoded.words);

    session.bit_write(COMMIT, true).unwrap();
    session.word_write(0x140f, SCAN_MS as u16).unwrap();
    session.bit_write(0x0c0f, true).unwrap();
    session.bit_write(STEP_MODE, true).unwrap();
    assert_eq!(session.word_read(PC).unwrap(), 0);

    let x0: Device = "X0".parse().unwrap();
    let y0: Device = "Y0".parse().unwrap();
    for input in [true, false] {
        session.bit_write(bit_address(x0).unwrap(), input).unwrap();
        let mut index = 0;
        while index < encoded.next_pc.len() {
            session.bit_write(STEP, true).unwrap();
            let actual_pc = session.word_read(PC).unwrap();
            index = checked_next_instruction(&encoded, index, actual_pc).unwrap();
        }
        assert_eq!(session.bit_read(y0).unwrap(), input);
        assert_eq!(session.word_read(0x1000).unwrap(), 123);
    }
}
