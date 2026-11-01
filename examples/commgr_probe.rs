//! Download through COMMGR: `commgr_probe DRIVER_ID [REMOTE_IP REMOTE_PORT] PROGRAM.il`.
//! COMMGR must be running with the driver configured; the target is treated as a simulator
//! (FC64 password/ID queries may be unsupported).
use opendvp::plc_download::{self, Options, Target};
use std::time::Duration;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (driver, remote, source) = match args.as_slice() {
        [driver, source] => (driver, None, source),
        [driver, ip, port, source] => (
            driver,
            Some((
                ip.parse().map_err(|e| format!("{e}"))?,
                port.parse::<u16>().map_err(|e| e.to_string())?,
            )),
            source,
        ),
        _ => return Err("Usage: commgr_probe DRIVER_ID [REMOTE_IP REMOTE_PORT] PROGRAM.il".into()),
    };
    let driver: u16 = driver.parse().map_err(|e| format!("{e}"))?;
    if let Some(path) = opendvp::commgr::driver_info_path()
        && let Ok(xml) = std::fs::read_to_string(&path)
    {
        println!("drivers: {:?}", opendvp::commgr::parse_drivers(&xml));
    }
    let text = std::fs::read_to_string(source).map_err(|e| e.to_string())?;
    let program = opendvp::plc::compile_il(&text).map_err(|e| format!("{e:?}"))?;
    let words = opendvp::es2_encoder::encode(&program)?.words;
    let mut link = opendvp::commgr::Link::connect(driver, remote, 1, Duration::from_secs(5))?;
    let report = plc_download::download(
        &mut link,
        &words,
        Options {
            target: Target::Simulator,
            run_after: true,
            ..Options::default()
        },
        |stage| eprintln!("{stage:?}"),
    )?;
    println!("report: {report:?}");
    println!("modules: {:?}", opendvp::plc_modules::scan(&mut link)?);
    Ok(())
}
