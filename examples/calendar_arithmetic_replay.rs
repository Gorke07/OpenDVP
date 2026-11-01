//! Replay original arithmetic outputs while preserving native PC defects.
use opendvp::{
    es2_encoder,
    plc::{Simulator, compile},
};
use serde_json::{Value, json};
fn main() {
    let input = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "rtc-arithmetic-native-fixtures.json".into());
    let evidence: Value = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let fixtures = evidence["fixtures"].as_array().unwrap();
    assert_eq!(fixtures.len(), 172);
    assert!(
        evidence["runs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["owned_child_closed"] == true && r["listener_pid_verified"] == true)
    );
    let mut checks = 0;
    let mut statuses = std::collections::BTreeMap::<String, usize>::new();
    for f in fixtures {
        let label = format!("{} {}", f["native_target"]["model"], f["name"]);
        let full = compile(f["source"].as_str().unwrap()).unwrap();
        let encoded = es2_encoder::encode(&full).unwrap();
        let words = f["words_hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| u16::from_str_radix(x.as_str().unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(encoded.words, words, "{label} exact native program");
        assert_eq!(
            full.native_action_pc[0],
            f["entry_pc"].as_u64().unwrap() as u16
        );
        // Native intentionally stops before its malformed continuation and
        // before the marker. The local isolated action uses the same ownerPC.
        let program = compile(&format!(
            "LD X0\n{} {} {} {}\nEND",
            f["op"].as_str().unwrap(),
            f["source1"].as_str().unwrap(),
            f["source2"].as_str().unwrap(),
            f["dest"].as_str().unwrap()
        ))
        .unwrap();
        assert_eq!(program.native_action_pc[0], full.native_action_pc[0]);
        let mut s = Simulator::default();
        s.memory.data_register_limit = Some(if f["native_target"]["model"] == "SS2" {
            4999
        } else {
            9999
        });
        for (address, value) in f["before"]["words"].as_object().unwrap() {
            s.memory.set_word(
                address.parse().unwrap(),
                value.as_u64().unwrap() as u16 as i16,
            );
        }
        for (index, value) in f["before"]["flags"].as_object().unwrap() {
            s.memory
                .set_system(index.parse().unwrap(), value.as_bool().unwrap());
        }
        for address in ["D1067", "D1068"] {
            s.memory.set_word(
                address.parse().unwrap(),
                f["before"][address].as_u64().unwrap() as u16 as i16,
            );
        }
        s.memory
            .set_input("X0".parse().unwrap(), f["power"].as_bool().unwrap());
        s.scan(&program, 0);
        for (address, value) in f["after"]["words"].as_object().unwrap() {
            assert_eq!(
                s.memory.word(address.parse().unwrap()) as u16,
                value.as_u64().unwrap() as u16,
                "{label} {address}"
            );
            checks += 1;
        }
        for (index, value) in f["after"]["flags"].as_object().unwrap() {
            assert_eq!(
                s.memory.system_bit(index.parse().unwrap()),
                value.as_bool().unwrap(),
                "{label} M{index}"
            );
            checks += 1;
        }
        for address in ["D1067", "D1068"] {
            assert_eq!(
                s.memory.word(address.parse().unwrap()) as u16,
                f["after"][address].as_u64().unwrap() as u16,
                "{label} {address}"
            );
            checks += 1;
        }
        assert!(s.memory.execution_limitations.is_empty());
        let status = f["status"].as_str().unwrap();
        *statuses.entry(status.into()).or_default() += 1;
        match status {
            "native_pc_defect_not_parity" => assert_eq!(f["actual_next_pc"], 4),
            "native_operation_error" | "native_disabled_nominal_pc" => {
                assert_eq!(f["actual_next_pc"], 8)
            }
            _ => panic!("Unexpected native status {status}"),
        }
        assert_eq!(f["expected_nominal_next_pc"], 8);
    }
    let result = json!({"scope":"Unmodified native arithmetic data/flags replay; retained successful-PC defects, not scan parity","fixtures":fixtures.len(),"checks":checks,"statuses":statuses});
    let output = serde_json::to_string_pretty(&result).unwrap();
    if let Some(path) = std::env::args().nth(2) {
        std::fs::write(path, output + "\n").unwrap();
    } else {
        println!("{output}");
    }
}
