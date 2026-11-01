//! Literal witnesses established before the independent native DCNT probes.
//! Only the installed C200..234 coil profile is under comparison here.
use super::{Case, case};
use opendvp::es2_bridge::{Reading, Target};

struct Scenario {
    name: &'static str,
    seed: i32,
    preset: i32,
    alternate: i32,
    dynamic: bool,
    // power, down, alternate preset, expected count, expected done
    steps: Vec<(bool, bool, bool, i32, bool)>,
}
fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "held_rearm",
            seed: 0,
            preset: 2,
            alternate: 2,
            dynamic: false,
            steps: vec![
                (true, false, false, 1, false),
                (true, false, false, 1, false),
                (false, false, false, 1, false),
                (true, false, false, 2, true),
                (true, false, false, 2, true),
                (false, false, false, 2, true),
                (true, false, false, 3, true),
            ],
        },
        Scenario {
            name: "direction_hold",
            seed: 1,
            preset: 2,
            alternate: 2,
            dynamic: false,
            steps: vec![
                (true, false, false, 2, true),
                (true, true, false, 2, true),
                (false, true, false, 2, true),
                (true, true, false, 1, false),
                (false, false, false, 1, false),
                (true, false, false, 2, true),
            ],
        },
        Scenario {
            name: "dynamic_preserves_contact",
            seed: 0,
            preset: 1,
            alternate: 3,
            dynamic: true,
            steps: vec![
                (true, false, false, 1, true),
                (true, false, true, 1, true),
                (false, false, true, 1, true),
                (true, false, true, 2, true),
                (false, false, true, 2, true),
                (true, false, true, 3, true),
            ],
        },
        Scenario {
            name: "zero_preset",
            seed: -1,
            preset: 0,
            alternate: 0,
            dynamic: false,
            steps: vec![
                (true, false, false, 0, true),
                (false, false, false, 0, true),
                (true, false, false, 1, true),
            ],
        },
        Scenario {
            name: "negative_preset",
            seed: -3,
            preset: -2,
            alternate: -2,
            dynamic: false,
            steps: vec![
                (true, false, false, -2, true),
                (false, true, false, -2, true),
                (true, true, false, -3, false),
            ],
        },
        Scenario {
            name: "up_wrap",
            seed: i32::MAX,
            preset: i32::MIN,
            alternate: i32::MIN,
            dynamic: false,
            steps: vec![
                (true, false, false, i32::MIN, true),
                (false, false, false, i32::MIN, true),
                (true, false, false, i32::MIN + 1, true),
            ],
        },
        Scenario {
            name: "down_wrap",
            seed: i32::MIN,
            preset: i32::MIN,
            alternate: i32::MIN,
            dynamic: false,
            steps: vec![
                (true, true, false, i32::MAX, false),
                (false, true, false, i32::MAX, false),
                (true, true, false, i32::MAX - 1, false),
            ],
        },
        Scenario {
            name: "negative_down",
            seed: -1,
            preset: -1,
            alternate: -1,
            dynamic: true,
            steps: vec![
                (true, true, false, -2, false),
                (true, false, false, -2, false),
                (false, false, false, -2, false),
                (true, false, false, -1, true),
            ],
        },
    ]
}
pub(super) fn cases(_target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    for counter in [200, 234] {
        for s in scenarios() {
            let preset = if s.dynamic {
                "D0".to_string()
            } else {
                format!("K{}", s.preset)
            };
            let source = format!(
                "LD M1002\nDMOV K{} C{counter}\nLD X1\nOUT M{}\nLD X2\nDMOV K{} D0\nLDI X2\nDMOV K{} D0\nLD X0\nDCNT C{counter} {preset}\nLD M1000\nDMOV C{counter} D20\nLD C{counter}\nOUT Y0\nEND",
                s.seed,
                1000 + counter,
                s.alternate,
                s.preset
            );
            let mut stages = vec![(vec![("X0", false), ("X1", false), ("X2", false)], 1)];
            stages.extend(s.steps.iter().map(|&(power, down, alternate, _, _)| {
                (vec![("X0", power), ("X1", down), ("X2", alternate)], 1)
            }));
            out.push(case(
                &format!("dcnt_c{counter}_{}", s.name),
                &source,
                stages,
            ));
        }
    }
    out
}
pub(super) fn validate_semantics(name: &str, scan: u64, rows: &[Reading]) -> Result<(), String> {
    let Some(rest) = name.strip_prefix("dcnt_c") else {
        return Ok(());
    };
    let (_, scenario) = rest.split_once('_').ok_or("DCNT witness name malformed")?;
    let s = scenarios()
        .into_iter()
        .find(|s| s.name == scenario)
        .ok_or("DCNT witness missing")?;
    let (value, done) = if scan == 1 {
        (s.seed, false)
    } else {
        let v = s
            .steps
            .get(scan as usize - 2)
            .ok_or("DCNT scan beyond literal witness")?;
        (v.3, v.4)
    };
    for (device, raw) in [
        ("D20", value as u32 & 0xffff),
        ("D21", (value as u32 >> 16) & 0xffff),
        ("Y0", u32::from(done)),
    ] {
        let d = device.parse::<opendvp::plc::Device>()?;
        let row = rows
            .iter()
            .find(|r| r.device == d)
            .ok_or_else(|| format!("{name}: missing {device}"))?;
        if row.local != raw || row.original != raw {
            return Err(format!(
                "{name} scan{scan} {device}: expected{raw}, local{}, native{}",
                row.local, row.original
            ));
        }
    }
    Ok(())
}
