//! Explicit caller-selected file-register image and recovery demonstration.
use opendvp::{
    file_memory::{FileMemoryProfile, FileModel, FileStore, FirmwareVersion, RecoverySource},
    plc::{Device, Simulator, compile},
};
use serde_json::json;
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn main() {
    let dir = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("Pass an explicit output directory"),
    );
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let path = dir.join("fixture-plc.odmem");
    let profile =
        FileMemoryProfile::new(FileModel::Es2, FirmwareVersion::new(3, 44), false).unwrap();
    let store = FileStore::new("file-memory-demo", profile.clone()).unwrap();
    let mut s = Simulator::default();
    s.memory.file_memory.configure(store);
    let p = compile("LD X0\nMEMW D2000 K10 K3\nLD X1\nMEMR K10 D2100 K3\nMEMR K4999 D2200 K1\nEND")
        .unwrap();
    for (index, value) in [(2000, 11), (2001, -2), (2002, 1234)] {
        s.memory.set_word(d(&format!("D{index}")), value);
    }
    s.memory.set_input(d("X0"), true);
    s.scan(&p, 0);
    s.memory.set_word(d("D2000"), 99);
    s.memory.set_input(d("X1"), true);
    s.scan(&p, 0);
    let held = s.memory.word(d("D2100"));
    assert_eq!(held, 11);
    assert_eq!(s.memory.word(d("D2101")), -2);
    assert_eq!(s.memory.word(d("D2200")), -1);
    s.memory.set_input(d("X0"), false);
    s.scan(&p, 0);
    s.memory.set_input(d("X0"), true);
    s.scan(&p, 0);
    let rearmed = s.memory.word(d("D2100"));
    assert_eq!(rearmed, 99);
    s.memory.file_memory.save(&path).unwrap();
    s.memory.set_input(d("X0"), false);
    s.scan(&p, 0);
    s.memory.set_word(d("D2000"), 321);
    s.memory.set_input(d("X0"), true);
    s.scan(&p, 0);
    s.memory.file_memory.save(&path).unwrap();
    assert!(s.memory.execution_limitations.is_empty());
    s.restart_preserving_file_store();
    assert_eq!(s.memory.word(d("D2100")), 0);
    s.memory.set_input(d("X1"), true);
    s.scan(&p, 0);
    let restarted = s.memory.word(d("D2100"));
    assert_eq!(restarted, 321);
    let loaded = FileStore::load_with_recovery(&path, "file-memory-demo", &profile).unwrap();
    assert_eq!(loaded.recovery_source, RecoverySource::Primary);
    assert_eq!(loaded.store.read_words(10, 1).unwrap(), [321]);
    std::fs::write(&path, b"intentionally corrupt local fixture primary").unwrap();
    let fallback = FileStore::load_with_recovery(&path, "file-memory-demo", &profile).unwrap();
    assert_eq!(fallback.recovery_source, RecoverySource::Previous);
    assert_eq!(fallback.store.read_words(10, 1).unwrap(), [99]);
    let repaired = FileStore::recover_previous(&path, "file-memory-demo", &profile).unwrap();
    assert_eq!(repaired.read_words(10, 1).unwrap(), [99]);
    let result = json!({"scope":"Explicit virtual file-register and durable host-image policy; not native/hardware flash parity","held_write_value":held,"rearmed_value":rearmed,"ram_restart_value":restarted,"erased_word":65535,"recovered_previous_value":99,"second_word_raw":65534,"transfer_words":3});
    let output = serde_json::to_string_pretty(&result).unwrap();
    if let Some(report) = std::env::args().nth(2) {
        std::fs::write(report, output + "\n").unwrap();
    } else {
        println!("{output}");
    }
}
