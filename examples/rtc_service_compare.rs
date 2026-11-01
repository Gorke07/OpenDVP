//! Small unchanged-image RTC transport qualification, before the bulk corpus.
//! Run only through the owned private-desktop launcher, never a user's GUI.
use opendvp::{
    es2_bridge::{RtcComparisonBudgets, Session},
    rtc_admission::{RtcControlFlags, RtcInitialInput},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("usage: rtc_service_compare SIMULATOR OUTPUT_JSON".into());
    }
    let target = Path::new(&args[0]);
    let output = Path::new(&args[1]);
    if output.exists() {
        return Err("Preserve existing RTC report".into());
    }
    let executable = std::env::current_exe()?;
    let producer_sha256 = format!("{:x}", Sha256::digest(fs::read(&executable)?));
    let mut results = Vec::new();
    let setup = "LDI X7\nMOV K4 D10\nMOV K2 D11\nMOV K12 D12\nMOV K15 D13\nMOV K15 D14\nMOV K27 D15\nMOV K30 D16\n";
    let cases = [
        (
            "rtc_trd_plain",
            "LD X0\nTRD D100\nEND".to_owned(),
            false,
            false,
        ),
        (
            "rtc_trdp_indexed",
            "LDI X7\nMOV K2 E1\nLD X0\nTRDP D100E1\nEND".to_owned(),
            false,
            false,
        ),
        (
            "rtc_twr_indexed",
            format!("{setup}LD X0\nTWR D10E0\nEND"),
            false,
            false,
        ),
        (
            "rtc_twrp_indexed",
            format!("{setup}LD X0\nTWRP D10F7\nEND"),
            false,
            false,
        ),
        (
            "rtc_four_digit_display",
            "LD X0\nTRD D100E0\nEND".to_owned(),
            true,
            false,
        ),
        (
            "rtc_level_minute_correction",
            "LD X0\nTRDP D100F0\nEND".to_owned(),
            false,
            true,
        ),
        (
            "rtc_twr_extent_error",
            "LD X0\nTWR T255E0\nEND".to_owned(),
            false,
            false,
        ),
        (
            "rtc_twr_bad_fields",
            "LDI X7\nFMOV K-1 D10 K7\nLD X0\nTWRP D10E0\nEND".to_owned(),
            false,
            false,
        ),
    ];
    let budgets = RtcComparisonBudgets {
        max_capture_us: 200_000,
        max_observation_us: 100_000,
        max_window_us: 250_000,
        max_between_us: 950_000,
        latest_start_phase_us: 600_000,
        continuous_host_clock_assumed: true,
    };
    let mut session = Session::launch(target, Arc::new(AtomicBool::new(false)))?;
    for (name, source, century, correction) in cases {
        let declared = RtcInitialInput {
            wire_seed: [24, 4, 2, 29, 12, 30, 37],
            controls: RtcControlFlags {
                m1016: century,
                m1017: correction,
                m1082: false,
            },
        };
        let mut scans = 0;
        let mut checks = 0;
        let result = (|| -> Result<(), String> {
            session.upload(&source)?;
            session.configure_rtc_comparison(declared, budgets)?;
            for on in [false, true, true, false, true] {
                let snapshot = session.step(&[("X0".parse().unwrap(), on)])?;
                if snapshot.mismatches != 0 {
                    return Err(format!(
                        "Unexpected accepted snapshot mismatches: {}",
                        snapshot.mismatches
                    ));
                }
                scans += 1;
                checks += snapshot.rows.len();
            }
            // A late seed is explicitly rejected, never used to heal output.
            if session.write_rtc_shadow([1; 7]).is_ok() {
                return Err("Late RTC seed was unexpectedly accepted".into());
            }
            Ok(())
        })();
        let status = match &result {
            Ok(()) => "match",
            Err(e) if e.contains("ConformanceFailure") => "conformance_failure",
            Err(_) => "inconclusive",
        };
        println!("{name}: {status}; {scans} scans, {checks} checks");
        results.push(json!({"name":name,"source":source,"declared":declared,"status":status,
            "scans":scans,"checks":checks,"error":result.err(),
            "verified_steps":session.verified_instruction_steps(),"verified_rtc_gates":session.verified_rtc_gates(),
            "rtc":session.rtc_comparison_evidence(),"trace":session.trace()}));
    }
    let matched = results.iter().filter(|r| r["status"] == "match").count();
    let failures = results
        .iter()
        .filter(|r| r["status"] == "conformance_failure")
        .count();
    let report = json!({"schema_version":1,"method":"Owned unchanged-image RTC settled service transport qualification; all attempts retained",
        "producer_executable":executable,"producer_sha256":producer_sha256,
        "probe_source_sha256":format!("{:x}",Sha256::digest(include_bytes!("rtc_service_compare.rs"))),
        "target":target,"target_sha256":format!("{:x}",Sha256::digest(fs::read(target)?)),
        "budgets":budgets,"matched":matched,"conformance_failures":failures,
        "inconclusive":results.len()-matched-failures,"cases":results});
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(serde_json::to_string_pretty(&report)?.as_bytes())?;
    if failures > 0 {
        std::process::exit(2);
    }
    if matched < 8 {
        std::process::exit(3);
    }
    Ok(())
}
