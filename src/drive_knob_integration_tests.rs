//! Compiled source checks against published packets and explicit virtual inputs.
use crate::analog_knob::KnobProfile;
use crate::plc::{Device, ScanStatus, Simulator, compile_il};
use crate::serial_io::{PortConfig, SerialPort};

fn d(name: &str) -> Device {
    name.parse().unwrap()
}
fn serial() -> Simulator {
    let mut s = Simulator::default();
    s.memory
        .serial_io
        .configure(SerialPort::Com2, PortConfig::rs485())
        .unwrap();
    s
}
fn scan(s: &mut Simulator, p: &crate::plc::Program, ms: u64) {
    s.scan(p, ms);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}

#[test]
fn nine_spellings_have_real_lengths_and_reject_native_stub_comparison() {
    for (source, header, words) in [
        ("FWD K1 K500 K0", 0xc600, 7),
        ("REV K1 K500 K0", 0xc800, 7),
        ("STOP K1 K0 K0", 0xca00, 7),
        ("RDST K1 K0", 0xcc00, 5),
        ("RSTEF K1 K0", 0xce00, 5),
        ("VRRD K0 D0", 0xba00, 5),
        ("VRRDP K0 D0", 0xba10, 5),
        ("VRSC K1 D0", 0xbc00, 5),
        ("VRSCP K1 D0", 0xbc10, 5),
    ] {
        let p = compile_il(&format!("LD X0\n{source}\nMOV K42 D200\nEND")).unwrap();
        let layout = crate::es2_encoder::local_execution_layout(&p).unwrap();
        assert_eq!(layout.known_word_at(1), Some(header), "{source}");
        assert_eq!(layout.action_pc, vec![1, 1 + words], "{source}");
        assert!(
            crate::es2_encoder::encode(&p)
                .unwrap_err()
                .contains("program sayacını")
        );
    }
    for source in [
        "FWDP K1 K500 K0",
        "DFWD K1 K500 K0",
        "RDSTP K1 K0",
        "DVRRD K0 D0",
        "RSTEF K1",
    ] {
        assert!(compile_il(&format!("LD X0\n{source}\nEND")).is_err());
    }
}

#[test]
fn source_sets_request_and_published_fwd_frame_completes_a_later_contact() {
    let p = compile_il("LD M1002\nMOV H86 D1120\nMOV K100 D1129\nSET M1177\nSET M1122\nLD M1000\nFWD K1 K500 K0\nLD M1127\nOUT Y0\nEND").unwrap();
    assert!(p.requires_vm());
    let mut s = serial();
    scan(&mut s, &p, 0);
    let tx = s.memory.serial_io.outgoing_frames().back().unwrap();
    assert_eq!(tx.bytes, b":01102000000204001201F4C2\r\n");
    let at = tx.completed_at_us + 2000;
    s.memory
        .serial_io
        .queue_received(SerialPort::Com2, at, b":011020000002CD\r\n".to_vec())
        .unwrap();
    scan(&mut s, &p, 50);
    scan(&mut s, &p, 0);
    assert!(s.memory.bit(d("Y0")));
    assert!(!s.memory.system_bit(1122));
    assert_eq!(s.memory.serial_io.outgoing_frames().len(), 1);
}

#[test]
fn ordinary_modbus_source_can_set_and_reset_its_documented_relays() {
    let p = compile_il("LD M1002\nMOV H86 D1120\nMOV K100 D1129\nRST M1143\nRST M1127\nSET M1122\nLD M1000\nMODWR K1 H2002 K2\nEND").unwrap();
    let mut s = serial();
    scan(&mut s, &p, 0);
    assert_eq!(
        s.memory.serial_io.outgoing_frames().back().unwrap().bytes,
        b":010620020002D5\r\n"
    );
    for index in [
        1120, 1121, 1122, 1123, 1124, 1125, 1126, 1127, 1128, 1129, 1130, 1131, 1136, 1139, 1140,
        1141, 1142, 1143, 1169, 1263, 1316, 1317, 1318, 1319, 1320,
    ] {
        assert!(format!("M{index}").parse::<Device>().is_ok());
    }
}

#[test]
fn compiled_knob_publication_is_read_only_and_restart_preserves_external_inputs() {
    let p = compile_il("LD M1000\nSET M1178\nMOV K999 D1178\nVRRD K0 D0\nVRSC K0 D1\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.analog_knob.set_profile(KnobProfile::VirtualSa2);
    s.memory.analog_knob.set_raw(0, 200).unwrap();
    s.memory.analog_knob.set_scaled_milli(0, 3400).unwrap();
    scan(&mut s, &p, 0);
    assert_eq!(
        (
            s.memory.word(d("D0")),
            s.memory.word(d("D1")),
            s.memory.word(d("D1178"))
        ),
        (200, 3, 200)
    );
    s.restart_preserving_file_store();
    scan(&mut s, &p, 0);
    assert_eq!(
        (
            s.memory.word(d("D0")),
            s.memory.word(d("D1")),
            s.memory.word(d("D1178"))
        ),
        (200, 3, 200)
    );
}

#[test]
fn scaled_pulse_in_control_flow_holds_and_rearms() {
    let p = compile_il("NOP\nLD X0\nVRSCP K1 D0\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.analog_knob.set_profile(KnobProfile::VirtualSx2);
    s.memory.analog_knob.set_graduation(1, 2).unwrap();
    s.memory.set_input(d("X0"), true);
    scan(&mut s, &p, 0);
    s.memory.analog_knob.set_graduation(1, 9).unwrap();
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.word(d("D0")), 2);
    s.memory.set_input(d("X0"), false);
    scan(&mut s, &p, 0);
    s.memory.set_input(d("X0"), true);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.word(d("D0")), 9);
}
