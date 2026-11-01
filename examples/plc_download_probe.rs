//! Live check of `plc_download` against an owned Delta simulator process.
//! Usage: plc_download_probe SIMULATOR.exe PROGRAM.il
//! Starts the simulator on a free loopback port, downloads the compiled program, puts the
//! simulator in RUN, toggles X0 and reports Y0, then terminates the simulator it started.
use opendvp::es2_protocol::Client;
use opendvp::plc_download::{self, Options, Target};
use std::net::TcpListener;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [simulator, source] = args.as_slice() else {
        return Err("Usage: plc_download_probe SIMULATOR.exe PROGRAM.il".into());
    };
    let text = std::fs::read_to_string(source).map_err(|e| e.to_string())?;
    let program = opendvp::plc::compile_il(&text).map_err(|e| format!("{e:?}"))?;
    let words = opendvp::es2_encoder::encode(&program)?.words;

    let port = TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map_err(|e| e.to_string())?
        .port();
    let dir = std::path::Path::new(simulator)
        .parent()
        .ok_or("simulator path")?;
    let mut child = Owned(
        Command::new(simulator)
            .arg(format!("OpenDVP;{port};"))
            .current_dir(dir)
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut client = loop {
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("simulator exited: {status}"));
        }
        match Client::connect(port, 1) {
            Ok(c) => break c,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e),
        }
    };

    let report = plc_download::download(
        &mut client,
        &words,
        Options {
            target: Target::Simulator,
            run_after: true,
            ..Options::default()
        },
        |stage| eprintln!("{stage:?}"),
    )?;
    println!("report: {report:?}");
    println!("modules: {:?}", opendvp::plc_modules::scan(&mut client)?);

    let mut y0 = Vec::new();
    for x0 in [true, false] {
        client.write_bit(0x0400, x0)?;
        std::thread::sleep(Duration::from_millis(300));
        y0.push(client.read_bits(0x0500, 1)?[0]);
    }
    let running = client.read_bits(0x0C30, 1)?[0];
    println!(
        "words={} running={running} X0 on/off -> Y0 {y0:?}",
        words.len()
    );
    Ok(())
}
