//! Explicit virtual Modbus transaction; no Windows COM port is opened.
//! Run: cargo run --example serial_modbus
use opendvp::plc::{Device, ScanStatus, Simulator, compile_il};
use opendvp::serial_io::{PortConfig, SerialPort};
use opendvp::serial_protocol::{VirtualModbusPeer, WireMode};

fn device(name: &str) -> Device {
    name.parse().expect("example device")
}

fn main() {
    let program =
        compile_il("LD X0\nMODRW K1 K3 H1000 D100 K2\nEND").expect("valid example program");
    let mut plc = Simulator::default();
    plc.memory
        .serial_io
        .configure(SerialPort::Com2, PortConfig::rs485())
        .expect("explicit COM2 virtual port");
    // Select 9600 baud / 8E1, RTU, 100 ms response timeout, and request.
    plc.memory.set_word(device("D1120"), 0x87);
    plc.memory.set_word(device("D1129"), 100);
    plc.memory.set_system(1143, true);
    plc.memory.set_system(1122, true);
    plc.memory.set_input(device("X0"), true);

    let mut peer = VirtualModbusPeer::new(1).expect("explicit station 1");
    peer.holding.extend([(0x1000, 0x1234), (0x1001, 0xabcd)]);
    plc.scan(&program, 0);
    assert_eq!(plc.last_scan, ScanStatus::Complete);
    opendvp::timing_io::advance_by(&mut plc.memory, 20_000);
    let tx = plc
        .memory
        .serial_io
        .pop_tx_frame()
        .expect("transmitted request");
    println!(
        "TX at {}..{} us: {:02X?}",
        tx.started_at_us, tx.completed_at_us, tx.bytes
    );
    let rx = peer
        .respond(WireMode::Rtu, &tx.bytes)
        .expect("valid frame")
        .expect("station 1 responds to read");
    println!("RX at 25000 us: {rx:02X?}");
    // This event supplies the bytes at the stated arrival time. The engine
    // then waits for the RTU end-of-frame silence before committing values.
    plc.memory
        .serial_io
        .queue_received(SerialPort::Com2, 25_000, rx)
        .expect("explicit future receive event");
    opendvp::timing_io::advance_by(&mut plc.memory, 15_000);
    assert!(
        plc.memory.execution_limitations.is_empty(),
        "{:?}",
        plc.memory.execution_limitations
    );
    assert!(plc.memory.system_bit(1127));
    assert_eq!(plc.memory.word(device("D1296")) as u16, 0x1234);
    assert_eq!(plc.memory.word(device("D1297")) as u16, 0xabcd);
    println!(
        "M1127=true; D1296=H1234; D1297=HABCD; completed by {} us",
        plc.memory.timing_io.now_us
    );
}
