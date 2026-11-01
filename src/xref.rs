//! Cross-reference: where each device is read or written in the program.
use opendvp::plc::{Action, Device, Program};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    pub network: usize,
    pub line: usize,
    pub write: bool,
    /// Instruction text (`OUT Y0`, `MOV K0 D0`) or `kontak` for a condition operand.
    pub instruction: String,
}

/// Uses per device, in network order.
pub fn uses(program: &Program) -> BTreeMap<Device, Vec<Use>> {
    let mut map: BTreeMap<Device, Vec<Use>> = BTreeMap::new();
    for (index, network) in program.networks.iter().enumerate() {
        let mut read = BTreeSet::new();
        network.condition.devices(&mut read);
        for device in read {
            map.entry(device).or_default().push(Use {
                network: index,
                line: network.line,
                write: false,
                instruction: "koşul (kontak / karşılaştırma)".into(),
            });
        }
        for action in &network.actions {
            let label = action.label();
            let written = action.written_device();
            if let Some(device) = written {
                map.entry(device).or_default().push(Use {
                    network: index,
                    line: network.line,
                    write: true,
                    instruction: label.clone(),
                });
            }
            let mut sources = BTreeSet::new();
            action.source_devices(&mut sources);
            for device in sources {
                if Some(device) == written {
                    continue;
                }
                map.entry(device).or_default().push(Use {
                    network: index,
                    line: network.line,
                    write: false,
                    instruction: label.clone(),
                });
            }
        }
    }
    map
}

/// Bit devices driven by `OUT` in more than one network ("double coil"): only the last
/// one decides the value at the end of the scan.
pub fn double_coils(program: &Program) -> BTreeSet<Device> {
    let mut seen: BTreeMap<Device, BTreeSet<usize>> = BTreeMap::new();
    for (index, network) in program.networks.iter().enumerate() {
        for action in &network.actions {
            if let Action::Out(device) = action {
                seen.entry(*device).or_default().insert(index);
            }
        }
    }
    seen.into_iter()
        .filter(|(_, networks)| networks.len() > 1)
        .map(|(device, _)| device)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_writes_and_double_coils_are_found() {
        let program = opendvp::plc::compile(
            "LD X0\nOR Y0\nANI X1\nOUT Y0\nLD Y0\nTMR T0 K30\nLD T0\nMOV K5 D0\nLD X2\nOUT Y0\nEND\n",
        )
        .unwrap();
        let map = uses(&program);
        let y0 = &map[&"Y0".parse().unwrap()];
        // Read in networks 0 and 1, written by OUT in networks 0 and 3.
        assert_eq!(y0.iter().filter(|u| u.write).count(), 2);
        assert_eq!(y0.iter().filter(|u| !u.write).count(), 2);
        let d0 = &map[&"D0".parse().unwrap()];
        assert_eq!(d0.len(), 1);
        assert!(d0[0].write && d0[0].instruction.contains("MOV"));
        assert!(map[&"T0".parse().unwrap()].iter().any(|u| u.write));
        assert_eq!(
            double_coils(&program).into_iter().collect::<Vec<_>>(),
            vec!["Y0".parse::<Device>().unwrap()]
        );
    }
}
