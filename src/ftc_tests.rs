use super::*;
use crate::plc::{ScanStatus, Simulator, compile_il};
fn action() -> FtcAction {
    FtcAction::parse(&["K1500", "D10", "D100", "D200"].map(str::to_owned)).unwrap()
}
fn memory() -> Memory {
    let mut m = Memory::default();
    m.ftc.profile = FtcProfile {
        model: FtcModel::Es2,
        firmware: FirmwareVersion::new(3, 22),
    };
    m.set_word(d(100), 60);
    m.set_word(d(101), 8);
    m.set_word(d(10), -25);
    m.set_word(d(200), 91);
    for i in 2..7 {
        m.set_word(d(100 + i), i as i16 * 11);
    }
    m
}
#[test]
fn genuine_codec_and_full_parameter_span_survive_without_fuzzy_claim() {
    let p = compile_il("LD X0\nFTC K1500 D10 D100 D200\nMOV K7 D300\nEND").unwrap();
    let l = crate::es2_encoder::local_execution_layout(&p).unwrap();
    assert_eq!(l.action_pc, vec![1, 10]);
    assert_eq!(l.word_count, 16);
    assert_eq!(l.known_word_at(1), Some(0x4201));
    assert!(l.unencoded_ranges.is_empty());
    assert!(crate::es2_encoder::encode(&p).is_err());
    for bad in [
        "FTCP K1 K2 D100 D200",
        "DFTC K1 K2 D100 D200",
        "FTC K1 K2 D9994 D200",
        "FTC T0 K2 D100 D200",
    ] {
        assert!(compile_il(&format!("LD X0\n{bad}\nEND")).is_err(), "{bad}");
    }
}
#[test]
fn unsupported_calculation_is_visible_and_preserves_all_state_and_mv() {
    let p = compile_il("LD X0\nFTC K1500 D10 D100 D200\nMOV K7 D300\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory = memory();
    s.memory.set_input("X0".parse().unwrap(), true);
    let before = (0..7)
        .map(|i| s.memory.word(d(100 + i)))
        .collect::<Vec<_>>();
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(s.memory.execution_limitations.contains(MISSING));
    assert_eq!(s.memory.word(d(200)), 91);
    assert_eq!(
        before,
        (0..7)
            .map(|i| s.memory.word(d(100 + i)))
            .collect::<Vec<_>>()
    );
    assert_eq!(s.memory.word(d(300)), 7);
}
#[test]
fn false_and_zero_sampling_hold_without_demanding_an_engine() {
    let mut m = memory();
    let a = action();
    a.execute(&mut m, false, 1);
    assert!(m.execution_limitations.is_empty());
    m.set_word(d(100), 0);
    a.execute(&mut m, true, 1);
    assert!(m.execution_limitations.is_empty());
    assert_eq!(m.word(d(200)), 91);
}
#[test]
fn config_priority_does_not_invent_gains_and_full_model_span_is_atomic() {
    let c = FtcConfig::decode(0xffc0 | 32 | 16 | 8 | 4 | 3);
    assert_eq!(c.environment, HeatingEnvironment::Slow);
    assert!(c.filter && c.fahrenheit);
    assert_eq!(c.reserved, 0xffc0);
    assert_eq!(
        FtcConfig::decode(0).environment,
        HeatingEnvironment::General
    );
    let a = FtcAction::parse(&["K1500", "D10", "D4995", "D200"].map(str::to_owned)).unwrap();
    let mut m = memory();
    m.data_register_limit = Some(4999);
    m.set_word(d(4995), 60);
    a.execute(&mut m, true, 1);
    assert_eq!(m.word(d(200)), 91);
    assert!(m.execution_limitations.iter().any(|e| e.contains("yedi")));
}
// Deliberately not a controller: literal commits test the adapter transaction,
// raw negative PV, timestamp and per-instruction opaque context only.
#[derive(Debug)]
struct LiteralAdapterFixture {
    invalid: bool,
}
impl FtcEngine for LiteralAdapterFixture {
    fn identity(&self) -> &str {
        "adapter-test-fixture; no Delta algorithm"
    }
    fn evaluate(&self, i: &FtcInput) -> Result<FtcStep, String> {
        assert_eq!(i.pv, -25);
        assert_eq!(i.sv, 1500);
        assert_eq!(i.parameters[0], 60);
        Ok(FtcStep {
            writes: vec![
                (FtcTarget::Parameter(2), 77),
                (FtcTarget::Output, if self.invalid { 6001 } else { 123 }),
            ],
            context: vec![i.context.first().copied().unwrap_or(0) + 1],
        })
    }
}
#[test]
fn external_adapter_commits_atomically_and_cpu_restart_discards_opaque_context() {
    let mut s = Simulator::default();
    s.memory = memory();
    s.memory
        .ftc
        .set_engine(Arc::new(LiteralAdapterFixture { invalid: true }))
        .unwrap();
    action().execute(&mut s.memory, true, 1);
    assert_eq!(s.memory.word(d(102)), 22);
    assert_eq!(s.memory.word(d(200)), 91);
    assert!(s.memory.ftc.loops.is_empty());
    s.memory
        .ftc
        .set_engine(Arc::new(LiteralAdapterFixture { invalid: false }))
        .unwrap();
    action().execute(&mut s.memory, true, 1);
    assert_eq!(s.memory.word(d(102)), 77);
    assert_eq!(s.memory.word(d(200)), 123);
    assert_eq!(s.memory.ftc.loops[&1].context, vec![1]);
    action().execute(&mut s.memory, true, 2);
    assert!(!s.memory.ftc.loops.contains_key(&2));
    s.restart_preserving_file_store();
    assert!(s.memory.ftc.loops.is_empty());
    assert!(s.memory.ftc.engine.is_some());
}
#[test]
fn explicitly_unsupported_ss2_and_earlier_firmware_do_not_run_external_engine() {
    for profile in [
        FtcProfile {
            model: FtcModel::Ss2,
            firmware: FirmwareVersion::new(9, 99),
        },
        FtcProfile {
            model: FtcModel::Es2,
            firmware: FirmwareVersion::new(3, 21),
        },
    ] {
        let mut m = memory();
        m.ftc.profile = profile;
        m.ftc
            .set_engine(Arc::new(LiteralAdapterFixture { invalid: false }))
            .unwrap();
        action().execute(&mut m, true, 1);
        assert_eq!(m.word(d(200)), 91);
        assert!(m.ftc.loops.is_empty());
        assert!(!m.execution_limitations.is_empty());
    }
}
