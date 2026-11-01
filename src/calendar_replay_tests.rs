//! Independent replay of retained, explicitly instrumented ES2/SS2 results.
//! Native host-delta suppression and tick injection are part of the fixture,
//! so this is controlled handler replay, not unmodified clock equivalence.
use super::*;
use serde_json::Value;

fn raw(value: &Value) -> u16 {
    value.as_u64().unwrap() as u16
}
fn data(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn shadow(value: &Value) -> [u8; 7] {
    std::array::from_fn(|i| raw(&value[i]) as u8)
}

#[test]
fn retained_controlled_calendar_handlers_match_both_models() {
    replay(
        &String::from_utf8(fixture!("rtc-controlled-replay-fixtures.json")).unwrap(),
        86,
        110,
    );
}

#[test]
fn retained_index_sources_match_controlled_native_handlers() {
    replay(
        &String::from_utf8(fixture!("rtc-index-replay-fixtures.json")).unwrap(),
        108,
        108,
    );
}

fn replay(text: &str, expected_fixtures: usize, expected_actions: usize) {
    let evidence: Value = serde_json::from_str(text).unwrap();
    let fixtures = evidence["fixtures"].as_array().unwrap();
    assert_eq!(fixtures.len(), expected_fixtures);
    let mut actions_checked = 0;
    for fixture in fixtures {
        let label = format!("{} {}", fixture["model"], fixture["name"]);
        let program = compile(fixture["source"].as_str().unwrap()).unwrap();
        let encoded = crate::es2_encoder::encode(&program).unwrap();
        let original_words = fixture["words_hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u16::from_str_radix(v.as_str().unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(encoded.words, original_words, "{label} compiler words");
        let actions = program
            .networks
            .iter()
            .flat_map(|n| &n.actions)
            .collect::<Vec<_>>();
        let observations = fixture["observations"].as_array().unwrap();
        assert_eq!(actions.len(), observations.len(), "{label}");
        let before = &observations[0]["before"];
        let mut memory = Memory {
            data_register_limit: Some(raw(&fixture["data_register_limit"])),
            ..Memory::default()
        };
        memory
            .rtc_clock
            .configure_frozen(shadow(&before["shadow_year_first"]));
        memory
            .hour_meter
            .select_profile(crate::hour_meter::HourProfile::InstalledTickBitmap);
        for (index, value) in before["words"].as_object().unwrap() {
            memory.set_word(data(index.parse().unwrap()), raw(value) as i16);
        }
        for (index, value) in before["flags"].as_object().unwrap() {
            memory.set_system(index.parse().unwrap(), raw(value) != 0);
        }
        for (i, value) in before["public_sec_to_year"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            memory.set_word(data(1313 + i as u16), raw(value) as i16);
        }
        memory.set_word(data(1067), raw(&before["D1067"]) as i16);
        if let Some(value) = before.get("D1068") {
            memory.set_word(data(1068), raw(value) as i16);
        }
        for (index, (action, observation)) in actions.into_iter().zip(observations).enumerate() {
            let before = &observation["before"];
            let after = &observation["after"];
            let pc = raw(&before["pc"]);
            assert_eq!(
                pc, program.native_action_pc[index],
                "{label} actual owner PC"
            );
            memory.native_pc = pc;
            memory
                .hour_meter
                .set_native_pending_ticks(raw(&before["pending_tick_mask"]) as u8);
            let power = observation["power"].as_bool().unwrap();
            match action {
                Action::Hour(a) => a.execute(&mut memory, power, pc, 0),
                Action::IndexedRtc(a) => {
                    a.execute(&mut memory, power, &mut std::collections::BTreeSet::new());
                }
                Action::Rtc(a) if power => {
                    a.execute(&mut memory);
                }
                Action::Rtc(_) => {}
                _ => panic!("Unexpected fixture action: {label}"),
            }
            for (index, value) in after["words"].as_object().unwrap() {
                assert_eq!(
                    memory.word(data(index.parse().unwrap())) as u16,
                    raw(value),
                    "{label} action{index} D{index}"
                );
            }
            for (index, value) in after["flags"].as_object().unwrap() {
                assert_eq!(
                    memory.system_bit(index.parse().unwrap()),
                    raw(value) != 0,
                    "{label} M{index}"
                );
            }
            for (i, value) in after["public_sec_to_year"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
            {
                assert_eq!(
                    memory.word(data(1313 + i as u16)) as u16,
                    raw(value),
                    "{label} RTC public offset{i}"
                );
            }
            assert_eq!(
                memory.rtc_clock.shadow(),
                Some(shadow(&after["shadow_year_first"])),
                "{label} private RTC"
            );
            assert_eq!(
                memory.word(data(1067)) as u16,
                raw(&after["D1067"]),
                "{label} error code"
            );
            if let Some(value) = after.get("D1068") {
                assert_eq!(
                    memory.word(data(1068)) as u16,
                    raw(value),
                    "{label} error location"
                );
            }
            assert_eq!(
                memory.hour_meter.pending_ticks,
                raw(&after["pending_tick_mask"]) as u8,
                "{label} pending ticks"
            );
            assert_eq!(
                memory.hour_meter.native_owners,
                std::array::from_fn(|i| raw(&after["owner_pcs"][i])),
                "{label} owner slots"
            );
            let instruction = encoded.next_pc.iter().position(|next| *next == pc).unwrap() + 1;
            assert_eq!(
                encoded.next_pc[instruction],
                raw(&after["pc"]),
                "{label} resulting PC"
            );
            assert!(
                memory.execution_limitations.is_empty(),
                "{label} {:?}",
                memory.execution_limitations
            );
            actions_checked += 1;
        }
    }
    assert_eq!(actions_checked, expected_actions);
}

#[test]
fn virtual_rtc_advances_inside_delay_and_hour_keeps_its_independent_clock() {
    let delays = "DELAY K1000\n".repeat(5);
    let p = compile(&format!(
        "LD X0\nTRD D0\n{delays}TRD D10\nHOUR K1 D100 M0\nEND"
    ))
    .unwrap();
    let mut s = Simulator::default();
    // This clock test deliberately executes 500 ms of DELAY in one scan.
    s.memory.set_word(data(1000), 1000);
    s.memory
        .rtc_clock
        .configure_virtual([2024, 3, 2, 28, 23, 59, 59])
        .unwrap();
    s.memory.rtc_clock.advance_monotonic_us(500000).unwrap();
    s.memory.set_input("X0".parse().unwrap(), true);
    s.scan(&p, 0);
    assert_eq!(s.memory.word(data(6)), 59);
    assert_eq!(
        (10..17).map(|i| s.memory.word(data(i))).collect::<Vec<_>>(),
        [24, 4, 2, 29, 0, 0, 0]
    );
    assert_eq!(s.memory.timing_io.now_us, 500000);
    assert_eq!(s.memory.word(data(101)), 0);
    s.scan(&p, 500);
    s.scan(&p, 0);
    assert_eq!(s.memory.word(data(101)), 1);
    assert_eq!(s.memory.rtc_clock.fraction_us(), 500000);
    assert!(s.memory.execution_limitations.is_empty());
}

#[test]
fn nominal_calendar_forms_and_native_clock_boundaries_are_explicit() {
    for (source, length, header) in [
        ("TCMP K1 K2 K3 D0 M0", 11, 0x6001),
        ("TCMPP K1 K2 K3 D0 M0", 11, 0x6011),
        ("TZCP D0 D3 D6 M0", 9, 0x6201),
        ("TZCPP D0 D3 D6 M0", 9, 0x6211),
        ("TADD D0 D3 D6", 7, 0x6401),
        ("TADDP D0 D3 D6", 7, 0x6411),
        ("TSUB D0 D3 D6", 7, 0x6601),
        ("TSUBP D0 D3 D6", 7, 0x6611),
        ("TRD D0", 3, 0x6a01),
        ("TRDP D0", 3, 0x6a11),
        ("TWR D0", 3, 0x6c01),
        ("TWRP D0", 3, 0x6c11),
        ("HOUR K1 D0 M0", 7, 0x7001),
        ("DHOUR K40000 D0 M0", 13, 0x7101),
    ] {
        let p = compile(&format!("LD X0\n{source}\nEND")).unwrap();
        let e = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(e.words.len(), length + 2, "{source}");
        assert_eq!(e.words[1], header, "{source}");
        assert_eq!(
            e.preflight.contains_key(&1),
            matches!(
                header,
                0x6401
                    | 0x6411
                    | 0x6601
                    | 0x6611
                    | 0x6a01
                    | 0x6a11
                    | 0x6c01
                    | 0x6c11
                    | 0x7001
                    | 0x7101
            )
        );
    }
    for source in [
        "DTCMP K1 K2 K3 D0 M0",
        "DTWR D0",
        "HOURP K1 D0 M0",
        "DHOURP K1 D0 M0",
    ] {
        assert!(compile(&format!("LD X0\n{source}\nEND")).is_err());
    }
}
