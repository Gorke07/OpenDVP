use opendvp::plc::{Simulator, compile_il};
use opendvp::serial_io::{PortConfig, SerialPort};
use opendvp::serial_protocol::{VirtualModbusPeer, WireMode};

fn main() -> Result<(), String> {
    let program = compile_il("LD M1002\nMOV H86 D1120\nMOV K100 D1129\nSET M1177\nSET M1122\nLD M1000\nFWD K1 K500 K0\nEND")
        .map_err(|e| format!("{e:?}"))?;
    let mut sim = Simulator::default();
    sim.memory
        .serial_io
        .configure(SerialPort::Com2, PortConfig::rs485())?;
    let mut peer = VirtualModbusPeer::new(1)?;
    peer.holding.insert(0x2000, 0);
    peer.holding.insert(0x2001, 0);
    sim.scan(&program, 0);
    let tx = sim
        .memory
        .serial_io
        .pop_tx_frame()
        .ok_or("Gönderim oluşmadı")?;
    assert_eq!(tx.bytes, b":01102000000204001201F4C2\r\n");
    let reply = peer
        .respond(WireMode::Ascii, &tx.bytes)?
        .ok_or("Yanıt oluşmadı")?;
    sim.memory
        .serial_io
        .queue_received(SerialPort::Com2, tx.completed_at_us + 2000, reply)?;
    sim.scan(&program, 50);
    assert!(sim.memory.execution_limitations.is_empty());
    assert!(sim.memory.system_bit(1127));
    assert_eq!(peer.holding[&0x2000], 0x12);
    assert_eq!(peer.holding[&0x2001], 500);
    println!("Sanal sürücü: H2000=H0012, H2001=500; M1127=ON");
    println!("Frekans birimi gerçek sürücünün modeline bağlıdır; fiziksel motor sürülmedi.");
    Ok(())
}
