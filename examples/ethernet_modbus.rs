//! Run an ETHRW request against an explicitly injected Modbus/TCP response.
use opendvp::ethernet::{EthernetModel, EthernetProfile};
use opendvp::file_memory::FirmwareVersion;
use opendvp::plc::{Device, Simulator, compile_il};
fn d(value: &str) -> Device {
    value.parse().unwrap()
}
fn main() {
    let mut s = Simulator::default();
    s.memory.ethernet.profile = EthernetProfile {
        model: EthernetModel::Se,
        firmware: FirmwareVersion::new(2, 4),
        ..Default::default()
    };
    s.memory.ethernet.configure_port(108).unwrap();
    for (index, value) in [
        (100, 2),
        (101, 0xc0a8u16 as i16),
        (102, 108),
        (103, 1),
        (104, 3),
    ] {
        s.memory.set_word(d(&format!("D{index}")), value);
    }
    s.memory.set_input(d("X0"), true);
    let p = compile_il("LD X0\nETHRW D100 H1000 D10 K2\nEND").unwrap();
    s.scan(&p, 0);
    let request = s.memory.ethernet.outgoing().back().unwrap().clone();
    assert_eq!(request.bytes, vec![0, 1, 0, 0, 0, 6, 1, 3, 0x10, 0, 0, 2]);
    s.memory
        .ethernet
        .queue_received(
            108,
            request.id,
            1000,
            vec![0, 1, 0, 0, 0, 7, 1, 3, 4, 0, 42, 0xff, 0xff],
        )
        .unwrap();
    opendvp::timing_io::advance_by(&mut s.memory, 1000);
    assert_eq!(s.memory.word(d("D10")), 42);
    assert_eq!(s.memory.word(d("D11")), -1);
    assert_eq!(s.memory.word(d("D1395")) as u16 & 0x100, 0x100);
    println!("ETHRW explicit virtual reply: D10=42, D11=-1, D1395 bit8=ON; no socket opened.");
}
