//! Online/offline comparison: the project's networks against the program read from the
//! PLC or simulator, matched by instruction content (longest common subsequence) so an
//! inserted or deleted network does not mark every later network as different.
use opendvp::plc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Same,
    Different,
    OnlyProject,
    OnlyPlc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub status: Status,
    /// Network index and normalized IL on each side.
    pub project: Option<(usize, Vec<String>)>,
    pub plc: Option<(usize, Vec<String>)>,
}

/// `Saved(12)` names the source line of an MPS; line numbers differ between the project
/// and a decoded PLC program, so the key keeps only `Saved`.
fn without_line_ids(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut rest = key;
    while let Some(at) = rest.find("Saved(") {
        out.push_str(&rest[..at + 6]);
        rest = rest[at + 6..].trim_start_matches(|c: char| c.is_ascii_digit());
    }
    out.push_str(rest);
    out
}

/// Every network as (logic key, display IL). Networks are equal when their compiled
/// logic is equal: the PLC stores `OR Y0` as `LD Y0 / ORB`, and both mean the same rung.
fn networks(source: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let program = plc::compile(source).map_err(|e| e.to_string())?;
    let lines: Vec<&str> = source.lines().collect();
    Ok(program
        .networks
        .iter()
        .map(|n| {
            let actions: Vec<String> = n.actions.iter().map(|a| a.label()).collect();
            let key = without_line_ids(&format!("{:?}|{actions:?}", n.condition));
            let display = lines[n.line.saturating_sub(1)..n.end_line.min(lines.len())]
                .iter()
                .map(|l| l.split(';').next().unwrap_or_default())
                .map(|l| {
                    l.split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_uppercase()
                })
                .filter(|l| !l.is_empty())
                .collect();
            (key, display)
        })
        .collect())
}

pub fn compare(project: &str, plc: &str) -> Result<Vec<Row>, String> {
    let a = networks(project).map_err(|e| format!("Proje derlenemedi: {e}"))?;
    let b = networks(plc).map_err(|e| format!("PLC programı derlenemedi: {e}"))?;
    // LCS table over whole-network equality.
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i].0 == b[j].0 {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut rows = Vec::new();
    let mut pending_a: Vec<usize> = Vec::new();
    let mut pending_b: Vec<usize> = Vec::new();
    // Unmatched runs between matches: pair them up as "different", the rest one-sided.
    let flush = |rows: &mut Vec<Row>, pa: &mut Vec<usize>, pb: &mut Vec<usize>| {
        let pairs = pa.len().max(pb.len());
        for k in 0..pairs {
            let left = pa.get(k).map(|&x| (x, a[x].1.clone()));
            let right = pb.get(k).map(|&y| (y, b[y].1.clone()));
            let status = match (&left, &right) {
                (Some(_), Some(_)) => Status::Different,
                (Some(_), None) => Status::OnlyProject,
                _ => Status::OnlyPlc,
            };
            rows.push(Row {
                status,
                project: left,
                plc: right,
            });
        }
        pa.clear();
        pb.clear();
    };
    while i < a.len() && j < b.len() {
        if a[i].0 == b[j].0 {
            flush(&mut rows, &mut pending_a, &mut pending_b);
            rows.push(Row {
                status: Status::Same,
                project: Some((i, a[i].1.clone())),
                plc: Some((j, b[j].1.clone())),
            });
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            pending_a.push(i);
            i += 1;
        } else {
            pending_b.push(j);
            j += 1;
        }
    }
    pending_a.extend(i..a.len());
    pending_b.extend(j..b.len());
    flush(&mut rows, &mut pending_a, &mut pending_b);
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserted_changed_and_missing_networks_are_reported() {
        let project = "; a\nLD X0\nOUT Y0\nLD X1\nOUT Y1\nLD X2\nOUT Y2\nEND\n";
        // PLC: network 2 changed (X1 → X5), plus an extra network at the end.
        let plc = "LD X0\nOUT Y0\nLD X5\nOUT Y1\nLD X2\nOUT Y2\nLD X3\nOUT Y3\nEND\n";
        let rows = compare(project, plc).unwrap();
        let statuses: Vec<Status> = rows.iter().map(|r| r.status).collect();
        assert_eq!(
            statuses,
            [
                Status::Same,
                Status::Different,
                Status::Same,
                Status::OnlyPlc
            ]
        );
        assert_eq!(rows[1].project.as_ref().unwrap().1, ["LD X1", "OUT Y1"]);
        // The PLC's block form of the same logic is equal.
        let blocks = "LD X0
LD Y0
ORB
LDI X1
ANB
OUT Y0
END
";
        let rows = compare(
            "LD X0
OR Y0
ANI X1
OUT Y0
END
",
            blocks,
        )
        .unwrap();
        assert_eq!(rows[0].status, Status::Same);
        // MPS branches match although the MPS line differs.
        let mps = "LD X0
MPS
AND X1
OUT Y0
MPP
AND X2
OUT Y1
END
";
        let rows = compare(
            &("; başlık

"
            .to_string()
                + mps),
            mps,
        )
        .unwrap();
        assert!(rows.iter().all(|r| r.status == Status::Same), "{rows:?}");
        // Comments and spacing do not matter.
        let same = compare(
            project,
            "LD   X0\nOUT Y0 ; x\nLD X1\nOUT Y1\nLD X2\nOUT Y2\nEND\n",
        );
        assert!(same.unwrap().iter().all(|r| r.status == Status::Same));
    }
}
