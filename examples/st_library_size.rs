//! Program words and jump labels of the ST library blocks.
//!
//! `cargo run --example st_library_size [NAME…]` prints per block: the words and labels one
//! more call adds when every call is expanded in place, and — for blocks whose instances can
//! share one body — the words one more call adds then and the size of the shared body.
//! Every input is bound to a program variable of its type, so no argument is a constant.

use opendvp::st::{LibraryEntry, LowerOptions, Share, library_entries};

fn source(entry: &LibraryEntry, calls: usize) -> String {
    let mut decls = String::new();
    for p in &entry.inputs {
        decls.push_str(&format!("  a_{} : {};\n", p.name, p.ty));
    }
    let args: Vec<String> = entry
        .inputs
        .iter()
        .map(|p| format!("{} := a_{}", p.name, p.name))
        .collect();
    let args = args.join(", ");
    let mut body = String::new();
    for i in 0..calls {
        match &entry.returns {
            Some(ty) => {
                decls.push_str(&format!("  r{i} : {ty};\n"));
                body.push_str(&format!("r{i} := {}({args});\n", entry.name));
            }
            None => {
                decls.push_str(&format!("  x{i} : {};\n", entry.name));
                body.push_str(&format!("x{i}({args});\n"));
            }
        }
    }
    format!("PROGRAM size\nVAR\n{decls}END_VAR\n{body}END_PROGRAM\n")
}

/// (words, labels, shared) of `calls` calls.
fn measure(entry: &LibraryEntry, calls: usize, share: Share) -> Result<(usize, usize, bool), String> {
    let src = source(entry, calls);
    let options = LowerOptions {
        share,
        ..LowerOptions::default()
    };
    let program = opendvp::plc::compile_st_with(&src, &options).map_err(|e| format!("{e:?}"))?;
    let words = opendvp::es2_encoder::encode(&program)
        .map_err(|e| e.to_string())?
        .words
        .len();
    let il = opendvp::st::lower_with(&src, &options)
        .map_err(|e| format!("{e:?}"))?
        .il;
    let labels = il
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() > 1 && w.starts_with('P') && w[1..].bytes().all(|b| b.is_ascii_digit()))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    Ok((words, labels, il.lines().any(|l| l == "FEND")))
}

fn main() {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    let (mut total_words, mut total_labels) = (0, 0);
    println!("{:<14} {:>6} {:>4}   {:>6} {:>6}", "block", "inline", "lbl", "shared", "body");
    for entry in &library_entries() {
        if !wanted.is_empty() && !wanted.iter().any(|w| w.eq_ignore_ascii_case(&entry.name)) {
            continue;
        }
        let inline = (measure(entry, 1, Share::Never), measure(entry, 2, Share::Never));
        let ((w1, l1, _), (w2, l2, _)) = match inline {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                println!("{:<14} error: {e}", entry.name);
                continue;
            }
        };
        total_words += w2 - w1;
        total_labels += l2 - l1;
        let shared = match (
            measure(entry, 2, Share::Always),
            measure(entry, 3, Share::Always),
        ) {
            (Ok((s2, _, true)), Ok((s3, _, true))) => {
                // Two calls = body + 2 per-call parts; the body is what remains.
                let per_call = s3 - s2;
                format!("{per_call:>6} {:>6}", (s2 - w1 + (w2 - w1)).saturating_sub(2 * per_call))
            }
            _ => format!("{:>6} {:>6}", "-", "-"),
        };
        println!("{:<14} {:>6} {:>4}   {shared}", entry.name, w2 - w1, l2 - l1);
    }
    println!("{:<14} {total_words:>6} {total_labels:>4}", "TOTAL");
}
