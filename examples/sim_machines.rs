//! Live walk-through on an owned Delta simulator: simple device commands first, then
//! program download / upload / delete, then machine programs driven and watched over time.
//! Usage: sim_machines SIMULATOR.exe
//! Exits non-zero when any expectation fails.
use opendvp::es2_protocol::Client;
use opendvp::plc_download::{self, Options, Target};
use std::net::TcpListener;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const X: u16 = 0x0400;
const Y: u16 = 0x0500;
const T: u16 = 0x0600;
const M: u16 = 0x0800;
const C: u16 = 0x0E00;
const D: u16 = 0x1000;
const RUN: u16 = 0x0C30;

struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Session {
    client: Client,
    start: Instant,
    passed: usize,
    failures: Vec<String>,
}

impl Session {
    fn check<V: PartialEq + std::fmt::Debug>(&mut self, what: &str, got: V, want: V) {
        if got == want {
            self.passed += 1;
            println!("    ok   {what}: {got:?}");
        } else {
            println!("    HATA {what}: okunan {got:?}, beklenen {want:?}");
            self.failures
                .push(format!("{what}: okunan {got:?}, beklenen {want:?}"));
        }
    }

    fn bit(&mut self, address: u16) -> Result<bool, String> {
        Ok(self.client.read_bits(address, 1)?[0])
    }

    fn set_input(&mut self, n: u16, on: bool) -> Result<(), String> {
        self.client.write_bit(X + n, on)
    }

    fn settle(&mut self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }

    /// One monitoring line: elapsed time, X0..X3, Y0..Y3, M0, T0/T1, C0, D0.
    fn snapshot(&mut self, note: &str) -> Result<(), String> {
        let x = self.client.read_inputs(X, 4)?;
        let y = self.client.read_bits(Y, 4)?;
        let m0 = self.bit(M)?;
        let t = self.client.read_words(T, 2)?;
        let c0 = self.client.read_words(C, 1)?[0];
        let d0 = self.client.read_words(D, 1)?[0];
        let bits = |v: &[bool]| {
            v.iter()
                .map(|b| if *b { '1' } else { '0' })
                .collect::<String>()
        };
        println!(
            "  {:>6.1}s  X0-3={} Y0-3={} M0={} T0={:>3} T1={:>3} C0={} D0={}  {note}",
            self.start.elapsed().as_secs_f32(),
            bits(&x),
            bits(&y),
            u8::from(m0),
            t[0],
            t[1],
            c0,
            d0
        );
        Ok(())
    }

    fn compile(path: &str) -> Result<Vec<u16>, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let program = opendvp::plc::compile_il(&text).map_err(|e| format!("{path}: {e:?}"))?;
        Ok(opendvp::es2_encoder::encode(&program)?.words)
    }

    fn download(&mut self, words: &[u16], run_after: bool) -> Result<(), String> {
        let report = plc_download::download(
            &mut self.client,
            words,
            Options {
                target: Target::Simulator,
                run_after,
                ..Options::default()
            },
            |_| {},
        )?;
        println!(
            "    indirildi: {} kelime, doğrulandı, RUN={}",
            report.words_written, report.running
        );
        Ok(())
    }

    /// Clear inputs and working devices so each machine starts from a known state.
    fn reset_devices(&mut self) -> Result<(), String> {
        self.client.write_bit(RUN, false)?;
        for n in 0..8 {
            self.client.write_bit(X + n, false)?;
            self.client.write_bit(Y + n, false)?;
        }
        self.client.write_bit(M, false)?;
        self.client.write_words(T, &[0, 0])?;
        self.client.write_bit(T, false)?;
        self.client.write_bit(T + 1, false)?;
        self.client.write_words(C, &[0])?;
        self.client.write_bit(C, false)?;
        self.client.write_words(D, &[0])?;
        Ok(())
    }

    fn machine(&mut self, title: &str, path: &str) -> Result<(), String> {
        println!("\n== {title} ({path})");
        let words = Self::compile(path)?;
        self.reset_devices()?;
        self.download(&words, true)?;
        self.settle(300);
        Ok(())
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("HATA: {e}");
        std::process::exit(1);
    }
}

fn start_simulator(path: &str) -> Result<(Owned, Client), String> {
    let port = TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map_err(|e| e.to_string())?
        .port();
    let dir = std::path::Path::new(path)
        .parent()
        .ok_or("simulator path")?;
    let mut child = Owned(
        Command::new(path)
            .arg(format!("OpenDVP;{port};"))
            .current_dir(dir)
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("simulator exited: {status}"));
        }
        match Client::connect(port, 1) {
            Ok(c) => return Ok((child, c)),
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e),
        }
    }
}

fn run() -> Result<(), String> {
    let simulator = std::env::args()
        .nth(1)
        .ok_or("Usage: sim_machines SIMULATOR.exe")?;
    let (_child, client) = start_simulator(&simulator)?;
    let mut s = Session {
        client,
        start: Instant::now(),
        passed: 0,
        failures: Vec::new(),
    };

    // ---------------------------------------------------------------- 1. simple commands
    println!("== 1. Basit komutlar (STOP)");
    let identity = plc_download::identify(&mut s.client)?;
    println!(
        "    kimlik: FC67={:?} kapasite={} adım",
        identity.info, identity.capacity_steps
    );
    s.client.write_bit(RUN, false)?;
    let running = s.bit(RUN)?;
    s.check("M1072 STOP", running, false);
    s.client.write_words(D, &[1234])?;
    let d0 = s.client.read_words(D, 1)?;
    s.check("D0 yaz/oku", d0, vec![1234]);
    s.client.write_words(D + 10, &[1, 2, 3, 4, 5])?;
    let block = s.client.read_words(D + 10, 5)?;
    s.check("D10..D14 blok", block, vec![1, 2, 3, 4, 5]);
    s.client.write_bit(M + 5, true)?;
    let m5 = s.bit(M + 5)?;
    s.check("M5 SET", m5, true);
    s.client.write_bit(M + 5, false)?;
    let m5 = s.bit(M + 5)?;
    s.check("M5 RST", m5, false);
    s.client.write_bit(Y + 3, true)?;
    let y3 = s.bit(Y + 3)?;
    s.check("Y3 zorla (STOP'ta)", y3, true);
    s.client.write_bit(Y + 3, false)?;
    s.set_input(7, true)?;
    let x7 = s.client.read_inputs(X + 7, 1)?;
    s.check("X7 giriş benzetimi", x7, vec![true]);
    s.set_input(7, false)?;
    s.client.write_words(T + 5, &[77])?;
    let t5 = s.client.read_words(T + 5, 1)?;
    s.check("T5 güncel değer", t5, vec![77]);
    s.client.write_words(C + 5, &[9])?;
    let c5 = s.client.read_words(C + 5, 1)?;
    s.check("C5 güncel değer", c5, vec![9]);
    let rtc = s.client.read_rtc_shadow()?;
    println!("    RTC (yıl, hafta günü, ay, gün, s, dk, sn): {rtc:?}");
    s.client.write_bit(RUN, true)?;
    let running = s.bit(RUN)?;
    s.check("M1072 RUN", running, true);
    s.client.write_bit(RUN, false)?;

    // ------------------------------------------------- 2. download / upload / delete
    println!("\n== 2. Program indir / geri oku / sil");
    let motor = Session::compile("examples/motor.il")?;
    s.reset_devices()?;
    s.download(&motor, true)?;
    let upload =
        plc_download::read_program(&mut s.client, identity.capacity_steps, motor.len(), |_| {})?;
    s.check("geri okunan program = indirilen", upload, motor.clone());
    s.set_input(0, true)?;
    s.settle(300);
    s.set_input(0, false)?;
    s.settle(300);
    let y0 = s.bit(Y)?;
    s.check("motor.il çalışıyor (X0 darbesi → Y0 mühürlü)", y0, true);
    // Delete = an END-only program: the PLC must stop reacting to the old logic.
    s.reset_devices()?;
    s.download(&[plc_download::PROGRAM_END], true)?;
    let first = s.client.read_words(0x2000, 1)?;
    s.check(
        "silindi: adım 0 = END",
        first,
        vec![plc_download::PROGRAM_END],
    );
    s.set_input(0, true)?;
    s.settle(300);
    let y0 = s.bit(Y)?;
    s.check("silinen program artık Y0 sürmüyor", y0, false);
    s.set_input(0, false)?;

    // ------------------------------------------------------------- 3. machines
    s.machine("3a. Su deposu", "examples/machines/water-tank.il")?;
    s.snapshot("depo boş (X0=0) → pompa çalışmalı")?;
    let y0 = s.bit(Y)?;
    s.check("boş depoda pompa", y0, true);
    s.set_input(0, true)?;
    s.settle(500);
    s.snapshot("alt seviye geçildi")?;
    s.set_input(1, true)?;
    s.settle(300);
    s.snapshot("depo doldu (X1=1)")?;
    let y0 = s.bit(Y)?;
    s.check("dolu depoda pompa durdu", y0, false);
    s.set_input(1, false)?;
    s.settle(300);
    let y0 = s.bit(Y)?;
    s.check(
        "üst seviye düşünce pompa beklemede (alt seviye hâlâ dolu)",
        y0,
        false,
    );
    s.set_input(0, false)?;
    s.settle(300);
    s.snapshot("su alt seviyenin altına indi → ikinci dolum")?;
    let c0 = s.client.read_words(C, 1)?;
    s.check("dolum sayısı C0", c0, vec![2]);
    for _ in 0..6 {
        s.settle(1000);
        s.snapshot("depo dolmuyor, alarm zamanlayıcısı sayıyor")?;
    }
    let (y0, y1) = (s.bit(Y)?, s.bit(Y + 1)?);
    s.check(
        "5 sn sonra alarm: pompa kapalı, Y1 yanık",
        (y0, y1),
        (false, true),
    );
    s.set_input(2, true)?;
    s.set_input(3, true)?;
    s.settle(300);
    s.set_input(3, false)?;
    s.settle(300);
    s.snapshot("acil stop basılı, alarm resetlendi")?;
    let (y0, y1) = (s.bit(Y)?, s.bit(Y + 1)?);
    s.check(
        "acil stop basılıyken pompa kapalı, alarm sönük",
        (y0, y1),
        (false, false),
    );
    s.set_input(2, false)?;
    s.settle(300);
    s.snapshot("acil stop bırakıldı")?;
    let y0 = s.bit(Y)?;
    s.check("acil stop bırakılınca pompa yeniden çalışır", y0, true);

    s.machine(
        "3b. Sıralı motorlar",
        "examples/machines/sequential-motors.il",
    )?;
    s.set_input(0, true)?;
    s.settle(200);
    s.set_input(0, false)?;
    let mut sequence = Vec::new();
    for step in 0..10 {
        s.settle(500);
        s.snapshot(&format!(
            "başlatıldıktan {:.1} sn sonra",
            0.2 + 0.5 * (step + 1) as f32
        ))?;
        sequence.push(s.client.read_bits(Y, 3)?);
    }
    s.check(
        "1 sn'de yalnız Y0",
        sequence[1].clone(),
        vec![true, false, false],
    );
    s.check(
        "3 sn'de Y0+Y1",
        sequence[5].clone(),
        vec![true, true, false],
    );
    s.check(
        "5 sn'de Y0+Y1+Y2",
        sequence[9].clone(),
        vec![true, true, true],
    );
    s.set_input(1, true)?;
    s.settle(300);
    s.snapshot("durdur basıldı")?;
    let ys = s.client.read_bits(Y, 3)?;
    s.check("durdurunca hepsi kapalı", ys, vec![false, false, false]);
    s.set_input(1, false)?;

    s.machine("3c. Yıldız-üçgen", "examples/machines/star-delta.il")?;
    s.set_input(0, true)?;
    s.settle(200);
    s.set_input(0, false)?;
    let mut overlap = false;
    let mut phases = Vec::new();
    let begin = Instant::now();
    while begin.elapsed() < Duration::from_millis(4500) {
        let ys = s.client.read_bits(Y, 3)?;
        overlap |= ys[1] && ys[2];
        let phase = match (ys[0], ys[1], ys[2]) {
            (true, true, false) => "yıldız",
            (true, false, false) => "ara",
            (true, false, true) => "üçgen",
            _ => "diğer",
        };
        if phases.last() != Some(&phase) {
            phases.push(phase);
            s.snapshot(&format!("faz: {phase}"))?;
        }
        s.settle(50);
    }
    s.check("faz sırası", phases.clone(), vec!["yıldız", "ara", "üçgen"]);
    s.check("yıldız ve üçgen hiç aynı anda yanmadı", overlap, false);
    s.set_input(2, true)?;
    s.settle(300);
    s.snapshot("termik attı (X2)")?;
    let ys = s.client.read_bits(Y, 3)?;
    s.check("termikte hepsi kapalı", ys, vec![false, false, false]);
    s.set_input(2, false)?;

    s.machine(
        "3d. Kutulama konveyörü",
        "examples/machines/box-conveyor.il",
    )?;
    s.set_input(0, true)?;
    s.settle(200);
    s.set_input(0, false)?;
    s.snapshot("konveyör başladı")?;
    for box_number in 1..=2 {
        for product in 1..=5 {
            s.set_input(2, true)?;
            s.settle(150);
            s.set_input(2, false)?;
            s.settle(150);
            if product == 5 {
                s.snapshot(&format!("kutu {box_number}: 5. ürün"))?;
            }
        }
        let (y0, y1) = (s.bit(Y)?, s.bit(Y + 1)?);
        s.check(
            "kutu dolunca konveyör durur, Y1 yanar",
            (y0, y1),
            (false, true),
        );
        s.set_input(2, true)?;
        s.settle(150);
        s.set_input(2, false)?;
        let c0 = s.client.read_words(C, 1)?;
        s.check("konveyör dururken gelen sinyal sayılmaz", c0, vec![5]);
        s.settle(3300);
        s.snapshot("3 sn sonra")?;
        let (y0, y1) = (s.bit(Y)?, s.bit(Y + 1)?);
        s.check(
            "sayaç sıfırlandı, konveyör yeniden çalışıyor",
            (y0, y1),
            (true, false),
        );
    }
    let d0 = s.client.read_words(D, 1)?;
    s.check("dolan kutu sayısı D0", d0, vec![2]);
    s.set_input(1, true)?;
    s.settle(300);
    let y0 = s.bit(Y)?;
    s.check("durdur", y0, false);

    s.client.write_bit(RUN, false)?;
    println!(
        "\nSonuç: {} denetim geçti, {} hata",
        s.passed,
        s.failures.len()
    );
    if s.failures.is_empty() {
        Ok(())
    } else {
        Err(s.failures.join("; "))
    }
}
