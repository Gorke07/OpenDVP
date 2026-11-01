use super::*;

fn id(text: &str) -> InterruptId {
    text.parse().unwrap()
}
fn linked(labels: &[&str]) -> InterruptState {
    let mut state = InterruptState::default();
    state
        .configure(
            labels
                .iter()
                .enumerate()
                .map(|(i, s)| (id(s), 10 + i as u16))
                .collect(),
            0,
        )
        .unwrap();
    state
}

#[test]
fn exact_label_spaces_and_timer_units_are_not_generic_i_numbers() {
    for label in [
        "I000", "I001", "I100", "I701", "I602", "I699", "I702", "I799", "I805", "I899", "I010",
        "I080", "I140", "I150", "I160",
    ] {
        let value = id(label);
        assert_eq!(value.to_string(), label);
        assert_eq!(value.word_len(), 1);
    }
    for invalid in [
        "I1", "I0000", "I 001", "I090", "I141", "I6010", "I800", "I804", "I900", "P001",
    ] {
        assert!(invalid.parse::<InterruptId>().is_err(), "{invalid}");
    }
    assert_eq!(id("I602").period_us(), Some(2000));
    assert_eq!(id("I805").period_us(), Some(500));
    assert_eq!(id("I001").period_us(), None);
}
#[test]
fn capabilities_and_timer_channel_ownership_are_checked_before_commit() {
    let mut state = linked(&["I610"]);
    state.on_ei([false; 3]);
    let previous = state.definitions().clone();
    assert!(
        state
            .configure([(id("I610"), 1), (id("I620"), 2)].into(), 0)
            .is_err()
    );
    assert_eq!(state.definitions(), &previous);
    assert!(state.enabled);
    state.capabilities = InterruptCapabilities::ss2_v2();
    assert!(state.configure([(id("I160"), 1)].into(), 0).is_err());
    state.capabilities = InterruptCapabilities::detail_5ms();
    assert!(state.configure([(id("I602"), 1)].into(), 0).is_err());
    assert!(state.configure([(id("I605"), 1)].into(), 0).is_ok());
}
#[test]
fn disabled_interval_latches_and_simultaneous_priority_uses_numeric_pointer() {
    let mut state = linked(&["I301", "I001", "I010"]);
    state.request(id("I301"), 10).unwrap();
    state.request(id("I010"), 10).unwrap();
    state.request(id("I001"), 10).unwrap();
    state.advance_to(10).unwrap();
    assert_eq!(state.dispatch([false; 10]), None);
    state.on_ei([false; 3]);
    for expected in ["I001", "I010", "I301"] {
        assert_eq!(state.dispatch([false; 10]).unwrap().0, id(expected));
        state.finish_handler().unwrap();
    }
}
#[test]
fn chronological_event_precedes_later_low_pointer_and_no_nesting_occurs() {
    let mut state = linked(&["I001", "I701"]);
    state.on_ei([false; 3]);
    state.request(id("I701"), 1).unwrap();
    state.request(id("I001"), 2).unwrap();
    state.advance_to(2).unwrap();
    assert_eq!(state.dispatch([false; 10]).unwrap().0, id("I701"));
    state.on_ei([false; 3]);
    assert_eq!(state.dispatch([false; 10]), None);
    state.on_di();
    state.finish_handler().unwrap();
    assert!(state.enabled);
    assert_eq!(state.dispatch([false; 10]).unwrap().0, id("I001"));
}
#[test]
fn explicit_return_policies_qualify_di_inside_handler_and_unmatched_iret() {
    for (policy, enabled) in [
        (ReturnEnable::RestoreEntry, true),
        (ReturnEnable::AlwaysEnable, true),
        (ReturnEnable::PreserveHandler, false),
    ] {
        let mut state = linked(&["I001"]);
        state.return_enable = policy;
        state.on_ei([false; 3]);
        state.request(id("I001"), 0).unwrap();
        state.dispatch([false; 10]).unwrap();
        state.on_di();
        state.finish_handler().unwrap();
        assert_eq!(state.enabled, enabled);
        assert!(state.finish_handler().is_err());
    }
}
#[test]
fn pending_coalescing_is_explicit_and_bounded_by_linked_ids() {
    let mut state = linked(&["I001"]);
    state.on_ei([false; 3]);
    for at in [20, 10, 30] {
        state.request(id("I001"), at).unwrap();
    }
    assert_eq!(
        state.pending()[&id("I001")],
        PendingEvent {
            at_us: 10,
            coalesced: 3
        }
    );
    assert_eq!(state.pending().len(), 1);
    state.pending_policy = PendingPolicy::Latest;
    state.request(id("I001"), 40).unwrap();
    assert_eq!(state.pending()[&id("I001")].at_us, 40);
    state.advance_to(39).unwrap();
    assert_eq!(state.dispatch([false; 10]), None);
    state.advance_to(40).unwrap();
    assert!(state.dispatch([false; 10]).is_some());
    assert!(state.request(id("I101"), 40).is_err());
}
#[test]
fn concepts_and_detail_masks_preserve_the_documented_conflict() {
    let mut flags = [false; 10];
    flags[5] = true;
    assert!(!id("I601").disabled(flags, MaskProfile::ConceptsTable));
    assert!(id("I601").disabled(flags, MaskProfile::InstructionDetail));
    flags = [false; 10];
    flags[6] = true;
    assert!(id("I601").disabled(flags, MaskProfile::ConceptsTable));
    assert!(!id("I601").disabled(flags, MaskProfile::InstructionDetail));
    assert!(id("I610").disabled(flags, MaskProfile::InstructionDetail));
    flags = [false; 10];
    flags[8] = true;
    assert!(!id("I160").disabled(flags, MaskProfile::ConceptsTable));
    flags[9] = true;
    assert!(id("I080").disabled(flags, MaskProfile::ConceptsTable));
}
#[test]
fn masked_event_retention_and_other_eligible_event_progress() {
    let mut state = linked(&["I001", "I101"]);
    state.on_ei([false; 3]);
    state.request(id("I001"), 0).unwrap();
    state.request(id("I101"), 0).unwrap();
    let mut flags = [false; 10];
    flags[0] = true;
    assert_eq!(state.dispatch(flags).unwrap().0, id("I101"));
    state.finish_handler().unwrap();
    assert_eq!(state.pending().len(), 1);
    assert_eq!(state.dispatch([false; 10]).unwrap().0, id("I001"));
}
#[test]
fn timer_deadlines_persist_across_end_configuration_and_di() {
    let mut state = linked(&["I602", "I805"]);
    state.advance_to(10_000).unwrap();
    assert_eq!(
        state.pending()[&id("I602")],
        PendingEvent {
            at_us: 2000,
            coalesced: 5
        }
    );
    assert_eq!(
        state.pending()[&id("I805")],
        PendingEvent {
            at_us: 500,
            coalesced: 20
        }
    );
    state
        .configure(state.definitions().clone(), 10_100)
        .unwrap();
    assert_eq!(state.pending()[&id("I805")].coalesced, 20);
    state.advance_to(10_500).unwrap();
    assert_eq!(state.pending()[&id("I805")].coalesced, 21);
    state.on_ei([false; 3]);
    assert_eq!(state.dispatch([false; 10]).unwrap().0, id("I805"));
}
#[test]
fn huge_timer_catchup_is_bounded_and_overflow_failure_does_not_mutate_phase() {
    let mut state = linked(&["I602"]);
    state.advance_to(1_000_000_000_000).unwrap();
    assert_eq!(state.pending().len(), 1);
    assert_eq!(state.pending()[&id("I602")].coalesced, 500_000_000);
    let before = state.now_us;
    let pending = state.pending().clone();
    assert!(state.advance_to(u64::MAX).is_err());
    assert_eq!(state.now_us, before);
    assert_eq!(state.pending(), &pending);
    assert!(state.advance_to(before - 1).is_err());
}
#[test]
fn physical_edges_and_ei_reverse_snapshot_do_not_use_software_input_filter() {
    let mut state = linked(&["I000", "I001"]);
    state.seed_input_levels([false; 8]);
    let x = Device {
        area: Area::X,
        index: 0,
    };
    state.input_edge(x, false, true, 0).unwrap();
    state.on_ei([false; 3]);
    assert_eq!(state.dispatch([false; 10]).unwrap().0, id("I001"));
    state.finish_handler().unwrap();
    state.on_di();
    state.on_ei([true, false, false]);
    state.input_edge(x, true, false, 1).unwrap();
    state.advance_to(1).unwrap();
    assert_eq!(state.dispatch([false; 10]).unwrap().0, id("I001"));
    assert!(
        state
            .input_edge(
                Device {
                    area: Area::X,
                    index: 15
                },
                false,
                true,
                2
            )
            .is_ok()
    );
}
#[test]
fn instruction_epoch_reset_clears_events_but_preserves_explicit_profile() {
    let mut state = linked(&["I001"]);
    state.header_instruction_cost_us = 7;
    state.mask_profile = MaskProfile::InstructionDetail;
    state.request(id("I001"), 0).unwrap();
    state.on_ei([true; 3]);
    state.clear_execution_epoch();
    assert!(!state.enabled);
    assert!(state.active.is_none());
    assert!(state.pending().is_empty());
    assert!(state.definitions().is_empty());
    assert_eq!(state.header_instruction_cost_us, 7);
    assert_eq!(state.mask_profile, MaskProfile::InstructionDetail);
}
#[test]
fn watchdog_strict_exceed_refresh_and_latched_timeout_are_distinct_from_budget() {
    let mut state = WatchdogState::default();
    assert!(state.enabled);
    state.begin_scan();
    state.advance(200_000, 200, 17).unwrap();
    assert!(state.fault.is_none());
    state.refresh(false);
    assert_eq!(state.elapsed_us, 200_000);
    state.refresh(true);
    assert_eq!(state.elapsed_us, 0);
    let fault = state.advance(200_001, 200, 31).unwrap_err();
    assert_eq!(
        fault,
        WatchdogError::Timeout {
            pc: 31,
            elapsed_us: 200_001,
            limit_us: 200_000
        }
    );
    state.refresh(true);
    state.begin_scan();
    assert_eq!(state.advance(0, 32767, 0), Err(fault));
    state.reset();
    state.advance(32_767_000, 32767, 1).unwrap();
    assert!(state.advance(1, 32767, 2).is_err());
}
#[test]
fn watchdog_zero_disabled_and_positive_delta_profiles_are_explicit() {
    let mut state = WatchdogState::default();
    assert_eq!(state.advance(0, 0, 0), Err(WatchdogError::InvalidLimit(0)));
    assert!(state.fault.is_none());
    assert_eq!(
        state.advance(1, -1, 0),
        Err(WatchdogError::InvalidLimit(-1))
    );
    state.zero_policy = ZeroWatchdog::Disabled;
    state.advance(u64::MAX, 0, 0).unwrap();
    assert_eq!(state.elapsed_us, 0);
    state.zero_policy = ZeroWatchdog::Immediate;
    assert!(state.advance(1, 0, 7).is_err());
    state.reset();
    state.profile = WatchdogProfile::Disabled;
    state.advance(u64::MAX, 1, 0).unwrap();
    assert_eq!(state.elapsed_us, 0);
    state.profile = WatchdogProfile::InstalledCostInput;
    state.enabled = false;
    state.advance(u64::MAX, 1, 0).unwrap();
    assert_eq!(state.elapsed_us, 0);
}
