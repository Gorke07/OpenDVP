//! Shared function-block bodies (one subroutine body for all instances, see `st/blocks.rs`)
//! behave exactly like calls expanded in place.
use crate::plc::{self, Device, Simulator};
use crate::st::{LibraryEntry, LowerOptions, Share, library_entries};

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    /// TRUE once in `n` calls.
    fn one_in(&mut self, n: u32) -> bool {
        self.next().is_multiple_of(n)
    }
}

/// A mapped variable of the test program: device and type name.
struct Mapped {
    device: Device,
    ty: String,
    name: String,
}

fn words(ty: &str) -> u16 {
    if matches!(ty, "DINT" | "DWORD" | "REAL") { 2 } else { 1 }
}

/// A program with `count` instances of `entry`: every input read from its own mapped
/// variable (bits from M100, words from D100) and every output copied to a mapped variable
/// (bits from M400, words from D1000).
fn program(entry: &LibraryEntry, count: usize) -> (String, Vec<Mapped>, Vec<Mapped>) {
    mixed(&[entry], count)
}

/// `count` instances of each of `entries` in one program, mapped as in `program`.
fn mixed(entries: &[&LibraryEntry], count: usize) -> (String, Vec<Mapped>, Vec<Mapped>) {
    let (mut in_bit, mut in_word, mut out_bit, mut out_word) = (100u16, 100u16, 400u16, 1000u16);
    let mut place = |ty: &str, input: bool| -> Device {
        let (bit, word) = if input {
            (&mut in_bit, &mut in_word)
        } else {
            (&mut out_bit, &mut out_word)
        };
        if ty == "BOOL" {
            *bit += 1;
            format!("M{}", *bit - 1).parse().unwrap()
        } else {
            *word += words(ty);
            format!("D{}", *word - words(ty)).parse().unwrap()
        }
    };
    let (mut decls, mut body, mut inputs, mut outputs) =
        (String::new(), String::new(), Vec::new(), Vec::new());
    for (e, entry) in entries.iter().enumerate() {
    for k in 0..count {
        let k = e * count + k;
        decls.push_str(&format!("    b{k} : {};\n", entry.name));
        let mut args = Vec::new();
        for p in &entry.inputs {
            let device = place(&p.ty, true);
            let name = format!("i{k}_{}", p.name);
            decls.push_str(&format!("    {name} AT {device} : {};\n", p.ty));
            args.push(format!("{} := {name}", p.name));
            inputs.push(Mapped {
                device,
                ty: p.ty.clone(),
                name,
            });
        }
        body.push_str(&format!("b{k}({});\n", args.join(", ")));
        for p in &entry.outputs {
            let device = place(&p.ty, false);
            let name = format!("o{k}_{}", p.name);
            decls.push_str(&format!("    {name} AT {device} : {};\n", p.ty));
            body.push_str(&format!("{name} := b{k}.{};\n", p.name));
            outputs.push(Mapped {
                device,
                ty: p.ty.clone(),
                name,
            });
        }
    }
    }
    (
        format!("PROGRAM P\nVAR\n{decls}END_VAR\n{body}END_PROGRAM\n"),
        inputs,
        outputs,
    )
}

/// Sets each input now and then: BOOLs toggle, numbers take a new value in a range that
/// suits times in ms, levels and setpoints alike.
fn drive(sim: &mut Simulator, inputs: &[Mapped], rng: &mut Rng) {
    for m in inputs {
        match m.ty.as_str() {
            "BOOL" => {
                if rng.one_in(6) {
                    let v = sim.memory.bit(m.device);
                    sim.memory.write_bit(m.device, !v);
                }
            }
            "REAL" => {
                if rng.one_in(12) {
                    let v = (rng.next() % 6000) as f32 / 10.0 - 50.0;
                    let bits = v.to_bits();
                    let next = Device {
                        area: m.device.area,
                        index: m.device.index + 1,
                    };
                    sim.memory.set_word(m.device, bits as u16 as i16);
                    sim.memory.set_word(next, (bits >> 16) as u16 as i16);
                }
            }
            _ => {
                if rng.one_in(12) {
                    let v = (rng.next() % 3200) as i16 - 100;
                    sim.memory.set_word(m.device, v);
                    if words(&m.ty) == 2 {
                        let next = Device {
                            area: m.device.area,
                            index: m.device.index + 1,
                        };
                        sim.memory.set_word(next, if v < 0 { -1 } else { 0 });
                    }
                }
            }
        }
    }
}

fn read(sim: &Simulator, m: &Mapped) -> Vec<i32> {
    if m.ty == "BOOL" {
        return vec![i32::from(sim.memory.bit(m.device))];
    }
    (0..words(&m.ty))
        .map(|i| {
            i32::from(sim.memory.word(Device {
                area: m.device.area,
                index: m.device.index + i,
            }))
        })
        .collect()
}

/// Runs the program inline and shared side by side for `scans` scans of 100 ms with the same
/// inputs and compares every output after every scan. Returns whether a body was shared.
fn same_behaviour(entry: &LibraryEntry, count: usize, scans: usize, seed: u32) -> bool {
    same_behaviour_of(&entry.name, program(entry, count), scans, seed)
}

fn same_behaviour_of(
    what: &str,
    (source, inputs, outputs): (String, Vec<Mapped>, Vec<Mapped>),
    scans: usize,
    seed: u32,
) -> bool {
    let options = |share| LowerOptions {
        share,
        ..LowerOptions::default()
    };
    let inline = plc::compile_st_with(&source, &options(Share::Never))
        .unwrap_or_else(|e| panic!("{what}: {e:?}\n{source}"));
    let shared_options = options(Share::Always);
    let lowered = crate::st::lower_with(&source, &shared_options).unwrap();
    if lowered.shared_blocks.is_empty() {
        return false;
    }
    let shared = plc::compile_st_with(&source, &shared_options)
        .unwrap_or_else(|e| panic!("{what}: {e:?}\n{}", lowered.il));
    let (mut a, mut b) = (Simulator::default(), Simulator::default());
    let mut rng = Rng(seed);
    for scan in 0..scans {
        let mut copy = Rng(rng.0);
        drive(&mut a, &inputs, &mut rng);
        drive(&mut b, &inputs, &mut copy);
        a.scan(&inline, 100);
        b.scan(&shared, 100);
        for m in &outputs {
            assert_eq!(
                read(&a, m),
                read(&b, m),
                "{what} scan {scan}: {} differs (inline vs shared)\n{}",
                m.name,
                lowered.il
            );
        }
    }
    true
}

/// Every library function block whose body can be shared gives the same outputs, scan for
/// scan, as its expanded calls: two instances, 250 scans of random inputs, two seeds.
#[test]
fn every_shareable_library_block_matches_its_inline_expansion() {
    let mut shared = Vec::new();
    for entry in library_entries().iter().filter(|e| e.kind == "FUNCTION_BLOCK") {
        let mut any = false;
        for seed in [0x9e37_79b9, 0x2545_f491] {
            any |= same_behaviour(entry, 2, 250, seed);
        }
        if any {
            shared.push(entry.name.clone());
        }
    }
    // Most blocks qualify; those that restart a timer inside an IF, keep arrays or are
    // retentive are expanded in place.
    assert!(shared.len() >= 50, "{} shared: {shared:?}", shared.len());
    for name in ["CYLINDER", "AXIS_2SPEED", "MOTOR", "VALVE", "TON_DSEC", "RECIPE_STEP", "OP_MODE"] {
        assert!(shared.iter().any(|s| s == name), "{name} is shared");
    }
}

/// Several shared bodies in one program: their temporaries, abort bits and index registers
/// do not disturb each other. The shareable blocks in groups of six, two instances each.
#[test]
fn mixed_shared_blocks_match_their_inline_expansion() {
    let entries = library_entries();
    let shareable: Vec<&LibraryEntry> = entries
        .iter()
        .filter(|e| e.kind == "FUNCTION_BLOCK")
        .filter(|e| {
            let options = LowerOptions {
                share: Share::Always,
                ..LowerOptions::default()
            };
            crate::st::lower_with(&program(e, 1).0, &options)
                .is_ok_and(|l| !l.shared_blocks.is_empty())
        })
        .collect();
    for group in shareable.chunks(6) {
        let names: Vec<&str> = group.iter().map(|e| e.name.as_str()).collect();
        assert!(same_behaviour_of(&names.join("+"), mixed(group, 2), 250, 0x5bd1_e995));
    }
}

/// The chosen form is the smaller one: five instances of a large block share its body, two
/// instances of a small one are expanded in place.
#[test]
fn sharing_is_chosen_only_where_it_saves_words() {
    if std::env::var("OPENDVP_ST_SHARE").is_ok() {
        return; // The default choice is overridden for the whole test run.
    }
    let entries = library_entries();
    let entry = |n: &str| entries.iter().find(|e| e.name == n).unwrap();
    let size = |src: &str, share| {
        let options = LowerOptions {
            share,
            ..LowerOptions::default()
        };
        let p = plc::compile_st_with(src, &options).unwrap();
        crate::es2_encoder::encode(&p).unwrap().words.len()
    };
    let (five, ..) = program(entry("CYLINDER"), 5);
    let lowered = crate::st::lower(&five).unwrap();
    assert_eq!(lowered.shared_blocks, ["CYLINDER"]);
    let (default, inline) = (size(&five, Share::Repeated), size(&five, Share::Never));
    assert!(default * 10 < inline * 7, "5 x CYLINDER: {default} shared, {inline} inline");
    let (two, ..) = program(entry("R_TRIG"), 2);
    assert!(crate::st::lower(&two).unwrap().shared_blocks.is_empty());
}

/// A body with REAL arithmetic leaves the scan from inside the subroutine through its abort
/// bit, as the expanded code jumps to the program end.
#[test]
fn a_fault_inside_a_shared_body_ends_the_scan() {
    let source = "FUNCTION_BLOCK Div
VAR_INPUT a : REAL; b : REAL; END_VAR
VAR_OUTPUT q : REAL; END_VAR
VAR t : TON; END_VAR
t(IN := TRUE, PT := 100);
q := a / b;
END_FUNCTION_BLOCK
PROGRAM P
VAR d1 : Div; d2 : Div; r1 AT D10 : REAL; r2 AT D12 : REAL; after AT D20 : INT; END_VAR
d1(a := 1.0, b := 0.0);
r1 := d1.q;
d2(a := 6.0, b := 3.0);
r2 := d2.q;
after := after + 1;
END_PROGRAM";
    for share in [Share::Never, Share::Always] {
        let options = LowerOptions {
            share,
            ..LowerOptions::default()
        };
        let lowered = crate::st::lower_with(source, &options).unwrap();
        assert_eq!(lowered.shared_blocks.is_empty(), share == Share::Never);
        let program = plc::compile_st_with(source, &options).unwrap();
        let mut sim = Simulator::default();
        for _ in 0..3 {
            sim.scan(&program, 100);
        }
        // The division by zero ends every scan before d2 and the counter run.
        assert_eq!(sim.memory.word("D20".parse().unwrap()), 0, "{share:?}\n{}", lowered.il);
        assert_eq!(sim.memory.word("D12".parse().unwrap()), 0, "{share:?}");
    }
}

/// An IL program of the project that uses E6 / E7 keeps every ST block expanded.
#[test]
fn an_il_program_using_e6_or_e7_turns_sharing_off() {
    use crate::project::{Project, SourceLanguage, UnitKind};
    let st = "PROGRAM U\nVAR c1 : CYLINDER; c2 : CYLINDER; c3 : CYLINDER; END_VAR\n\
              c1(EXTEND := X0);\nc2(EXTEND := X1);\nc3(EXTEND := X2);\nEND_PROGRAM\n";
    for (main, shared) in [
        ("LD X7\nOUT Y7\nEND\n", true),
        ("LD X7\nMOV K3 E6\nEND\n", false),
        ("LD X7\nMOV D0E7 D20\nEND\n", false),
    ] {
        let mut p = Project {
            source: main.into(),
            ..Project::default()
        };
        let i = p.add_unit(UnitKind::Cyclic).unwrap();
        p.set_unit_language(i, SourceLanguage::St).unwrap();
        p.set_program_source(i, st.into());
        let il = p.instruction_list().unwrap();
        assert_eq!(il.lines().any(|l| l == "SRET"), shared, "{main}\n{il}");
    }
}

/// Instances of a shared block in two ST programs of one project keep their own variables,
/// and the shared bodies are placed after FEND.
#[test]
fn shared_bodies_of_two_project_programs_stay_apart() {
    use crate::project::{Project, SourceLanguage, UnitKind};
    let unit = |out: &str, x: &str| {
        format!(
            "PROGRAM U\nVAR c1 : CYLINDER; c2 : CYLINDER; c3 : CYLINDER; q AT {out} : BOOL; END_VAR\n\
             c1(EXTEND := {x});\nc2(EXTEND := FALSE);\nc3(EXTEND := FALSE);\n\
             q := c1.EXT_OUT;\nEND_PROGRAM\n"
        )
    };
    let mut p = Project {
        source: "LD X7\nOUT Y7\nEND\n".into(),
        ..Project::default()
    };
    for (out, x) in [("Y0", "X0"), ("Y1", "X1")] {
        let i = p.add_unit(UnitKind::Cyclic).unwrap();
        p.set_unit_language(i, SourceLanguage::St).unwrap();
        p.set_program_source(i, unit(out, x));
    }
    let il = p.instruction_list().unwrap();
    let fend = il.lines().position(|l| l == "FEND").expect("FEND");
    assert_eq!(il.lines().skip(fend).filter(|l| *l == "SRET").count(), 4, "two bodies of two segments after FEND:\n{il}");
    assert!(!il.lines().take(fend).any(|l| l == "SRET"), "no body before FEND:\n{il}");
    let program = p.compile().unwrap_or_else(|e| panic!("{e:?}\n{il}"));
    let mut sim = Simulator::default();
    sim.scan(&program, 100);
    // A command already present at power-up is ignored; a new one extends the cylinder.
    sim.memory.write_bit("X0".parse().unwrap(), true);
    for _ in 0..3 {
        sim.scan(&program, 100);
    }
    let bit = |n: &str| sim.memory.bit(n.parse().unwrap());
    assert!(bit("Y0") && !bit("Y1"), "only the first program's cylinder extends\n{il}");
}
