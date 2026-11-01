//! Delta editor project (`.isp`) reader: unpacks the project text and converts IL and LD
//! POUs to OpenDVP IL (ST POUs to Structured Text).
//!
//! Container (Delta editor `editor.exe` save routine `routine`; see
//! ), little-endian:
//!
//! | offset | content |
//! |---|---|
//! | 0x10 | u32 device code (DVP models use the `.dvz` model index numbering) |
//! | 0x78 | u32 ANSI code page of the saving PC (0 in older saves) |
//! | 0x94 | u32 compressed-source length |
//! | 0x98 | u32 compressed-source offset (0xA4 + compiled-code length) |
//! | 0xA4 | compiled code (not used here) |
//! | [0x98] | u16 method (8 = deflate, 0 = stored), u32 CRC32 of the text, raw deflate |
//!
//! LD networks are flat node lists; the tree is rebuilt with Delta editor's own rules
//! (`routine`): node type 7 closes a 3-node box, types 6/29 take `LNK_C` branches of
//! `LNK_L` cells from the nodes before them.

const HEADER: usize = 0xA4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub language: crate::project::SourceLanguage,
    pub source: String,
    /// POUs or nodes that could not be converted.
    pub notes: Vec<String>,
    /// `(device, comment)` from the project's device comment table.
    pub device_comments: Vec<(String, String)>,
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Decompressed project text.
pub fn project_text(data: &[u8]) -> Result<String, String> {
    if data.len() < HEADER {
        return Err("Delta editor .isp başlığı eksik".into());
    }
    let zip_size = u32_at(data, 0x94) as usize;
    let zip_start = u32_at(data, 0x98) as usize;
    let block = if zip_size == 0 {
        data.get(zip_start..)
    } else {
        data.get(zip_start..zip_start.saturating_add(zip_size))
    }
    .filter(|b| b.len() >= 6)
    .ok_or("Delta editor proje metni bulunamadı")?;
    let method = u16::from_le_bytes([block[0], block[1]]);
    let crc = u32::from_le_bytes(block[2..6].try_into().unwrap());
    let text = match method {
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(&block[6..], 16 * 1024 * 1024)
            .map_err(|e| format!("Delta editor proje metni açılamadı: {e:?}"))?,
        0 => block[6..].to_vec(),
        other => return Err(format!("Bilinmeyen Delta editor sıkıştırma yöntemi {other}")),
    };
    if crc32(&text) != crc {
        return Err("Delta editor proje metninin CRC değeri tutmuyor".into());
    }
    let code_page = u32_at(data, 0x78);
    let encoding = match code_page {
        0 if std::str::from_utf8(&text).is_ok() => encoding_rs::UTF_8,
        // Older saves do not record the code page; Delta's bundled examples are Big5.
        0 | 950 => encoding_rs::BIG5,
        936 => encoding_rs::GBK,
        932 => encoding_rs::SHIFT_JIS,
        949 => encoding_rs::EUC_KR,
        65001 => encoding_rs::UTF_8,
        cp => encoding_rs::Encoding::for_label(format!("windows-{cp}").as_bytes())
            .unwrap_or(encoding_rs::WINDOWS_1252),
    };
    Ok(encoding.decode(&text).0.into_owned())
}

#[derive(Debug, Clone, Default)]
struct Var {
    name: String,
    kind: String,
    address: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct Node {
    kind: i32,
    device: String,
    symbol: String,
    branches: usize,
    cells: usize,
    vars: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct Network {
    comment: String,
    root: Vec<Node>,
    out: Vec<Node>,
}

#[derive(Debug, Clone, Default)]
struct Pou {
    name: String,
    lang: i32,
    comment: String,
    vars: Vec<Var>,
    code: Option<String>,
    networks: Vec<Network>,
}

#[derive(Debug, Default)]
struct ProjectText {
    globals: Vec<Var>,
    task_order: Vec<String>,
    device_comments: Vec<(String, String)>,
    pous: Vec<Pou>,
}

/// Lines between `lines[i]` and `end`, plus the index of `end`.
fn block<'a>(lines: &[&'a str], i: usize, end: &str) -> (Vec<&'a str>, usize) {
    let mut j = i + 1;
    while j < lines.len() && lines[j] != end {
        j += 1;
    }
    (lines[i + 1..j.min(lines.len())].to_vec(), j)
}

/// `(* ... *)` starting at `lines[i]`, possibly over several lines.
fn comment(lines: &[&str], i: usize) -> (Option<String>, usize) {
    let Some(first) = lines.get(i).filter(|l| l.starts_with("(*")) else {
        return (None, i);
    };
    if first.ends_with("*)") && first.len() > 3 {
        return (Some(first[2..first.len() - 2].trim().to_owned()), i + 1);
    }
    let mut body = vec![first[2..].to_owned()];
    let mut j = i + 1;
    while j < lines.len() && lines[j] != "*)" {
        body.push(lines[j].to_owned());
        j += 1;
    }
    (Some(body.join("\n").trim().to_owned()), j + 1)
}

/// `{` / `NAME : TYPE := init [@ADDRESS@] {CLASS}` / optional comment / `}`.
fn parse_vars(lines: &[&str]) -> Vec<Var> {
    let mut vars = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i] != "{" {
            i += 1;
            continue;
        }
        if let Some(line) = lines.get(i + 1)
            && let Some((name, rest)) = line.split_once(':')
        {
            let kind = rest
                .split(":=")
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned();
            let address = line
                .split_once("[@")
                .and_then(|(_, a)| a.split_once("@]"))
                .map(|(a, _)| a.trim().to_owned())
                .filter(|a| !a.is_empty() && !a.starts_with("N/A"));
            vars.push(Var {
                name: name.trim().to_owned(),
                kind,
                address,
            });
        }
        let mut k = i + 1;
        while k < lines.len() && lines[k] != "}" {
            k += 1;
        }
        i = k + 1;
    }
    vars
}

fn parse_node(lines: &[&str]) -> Node {
    let mut node = Node {
        kind: -1,
        branches: 2,
        cells: 1,
        ..Node::default()
    };
    let mut in_vars = false;
    for line in lines {
        match *line {
            "<VAR_NODE_S>" => in_vars = true,
            "<VAR_NODE_E>" => in_vars = false,
            _ if in_vars => {
                if let Some(v) = line.strip_prefix("VAR_NAME=") {
                    node.vars.push(v.to_owned());
                }
            }
            _ => match line.split_once('=') {
                Some(("TYPE", v)) => node.kind = v.trim().parse().unwrap_or(-1),
                Some(("DEV_NAME", v)) => node.device = v.to_owned(),
                Some(("SYMB", v)) => node.symbol = v.to_owned(),
                Some(("LNK_C", v)) => node.branches = v.trim().parse().unwrap_or(2),
                Some(("LNK_L", v)) => node.cells = v.trim().parse().unwrap_or(1),
                _ => {}
            },
        }
    }
    node
}

fn parse_nodes(lines: &[&str]) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i] == "[LD_NODE]" {
            let (body, j) = block(lines, i, "[END_LD_NODE]");
            nodes.push(parse_node(&body));
            i = j + 1;
        } else {
            i += 1;
        }
    }
    nodes
}

fn parse_network(lines: &[&str]) -> Network {
    let mut net = Network::default();
    let mut k = 0;
    while k < lines.len() {
        match lines[k] {
            "<PROPERTIES_START>" => {
                let (props, end) = block(lines, k, "<PROPERTIES_END>");
                let mut p = 0;
                while p < props.len() {
                    if props[p].starts_with("(*") {
                        let (text, next) = comment(&props, p);
                        net.comment = text.unwrap_or_default();
                        p = next;
                    } else {
                        p += 1;
                    }
                }
                k = end;
            }
            "<ROOTLINK_START>" => {
                let (body, end) = block(lines, k, "<ROOTLINK_END>");
                net.root = parse_nodes(&body);
                k = end;
            }
            "<OUTLINK_START>" => {
                let (body, end) = block(lines, k, "<OUTLINK_END>");
                net.out = parse_nodes(&body);
                k = end;
            }
            _ => {}
        }
        k += 1;
    }
    net
}

fn parse_pou(lines: &[&str]) -> Pou {
    let mut pou = Pou {
        lang: -1,
        ..Pou::default()
    };
    let mut commented = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("(*") && !commented {
            let (text, next) = comment(lines, i);
            pou.comment = text.unwrap_or_default();
            commented = true;
            i = next;
            continue;
        }
        match line {
            "<LOCAL_VAR>" => {
                let (body, end) = block(lines, i, "</LOCAL_VAR>");
                pou.vars = parse_vars(&body);
                i = end;
            }
            "<IL_ST_CODE>" => {
                let (body, end) = block(lines, i, "</IL_ST_CODE>");
                pou.code = Some(body.join("\n"));
                i = end;
            }
            // SFC/FBD bodies are not converted; skip them so their networks are not
            // mistaken for LD networks of the POU.
            "<NODE_DATA>" | "<ACT_LIST>" | "<TRANS_LIST>" => {
                let end_tag = format!("</{}", &line[1..]);
                i = block(lines, i, &end_tag).1;
            }
            "<NETWORK_START>" => {
                let (body, end) = block(lines, i, "<NETWORK_END>");
                if !body.iter().any(|l| l.starts_with("[NODE_ID]")) {
                    pou.networks.push(parse_network(&body));
                }
                i = end;
            }
            _ => match line.split_once('=') {
                Some(("P_Name", v)) => pou.name = v.to_owned(),
                Some(("P_Lang", v)) => pou.lang = v.trim().parse().unwrap_or(-1),
                _ => {}
            },
        }
        i += 1;
    }
    pou
}

fn parse_project(text: &str) -> ProjectText {
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    let mut project = ProjectText::default();
    let mut i = 0;
    while i < lines.len() {
        match lines[i] {
            "<TASK>" => {
                let (body, end) = block(&lines, i, "</TASK>");
                for line in body {
                    if let Some(pou) = line.strip_prefix("T_PName=") {
                        project.task_order.push(pou.to_owned());
                    }
                }
                i = end;
            }
            "<GLOBAL_VAR>" => {
                let (body, end) = block(&lines, i, "</GLOBAL_VAR>");
                project.globals = parse_vars(&body);
                i = end;
            }
            "[DEVICE_CMT_START]" => {
                let (body, end) = block(&lines, i, "[DEVICE_CMT_END]");
                for line in body {
                    if let Some((device, text)) = line.split_once("::") {
                        project
                            .device_comments
                            .push((device.to_owned(), text.to_owned()));
                    }
                }
                i = end;
            }
            "<POU>" => {
                let (body, end) = block(&lines, i, "</POU>");
                project.pous.push(parse_pou(&body));
                i = end;
            }
            _ => {}
        }
        i += 1;
    }
    project
}

// ------------------------------------------------------------------- LD tree → IL

#[derive(Debug, Clone)]
enum Item {
    Node(Node),
    Box {
        inputs: Node,
        op: Node,
        outputs: Node,
    },
    Par {
        kind: i32,
        cells: usize,
        branches: Vec<Vec<Item>>,
    },
}

const FILLER: i32 = 20;

fn contact_forms(kind: i32) -> Option<[&'static str; 3]> {
    Some(match kind {
        1 => ["LD", "AND", "OR"],
        2 => ["LDI", "ANI", "ORI"],
        3 => ["LDP", "ANDP", "ORP"],
        4 => ["LDF", "ANDF", "ORF"],
        _ => return None,
    })
}
fn coil(kind: i32) -> Option<&'static str> {
    Some(match kind {
        13 | 14 => "OUT",
        15 => "SET",
        16 => "RST",
        _ => return None,
    })
}
fn standalone(kind: i32) -> Option<&'static str> {
    Some(match kind {
        24 => "NP",
        25 => "PN",
        26 => "INV",
        _ => return None,
    })
}

fn width(item: &Item) -> usize {
    match item {
        Item::Node(_) => 1,
        Item::Box { .. } => 3,
        Item::Par { cells, .. } => *cells,
    }
}

fn build_tree(nodes: &[Node], notes: &mut Vec<String>) -> Vec<Item> {
    let mut stack: Vec<Item> = Vec::new();
    for node in nodes {
        match node.kind {
            7 => {
                if stack.len() < 3 {
                    notes.push("LD: kutu sonu için 3 düğüm yok".into());
                    continue;
                }
                let outputs = stack.pop();
                let op = stack.pop();
                let inputs = stack.pop();
                match (inputs, op, outputs) {
                    (Some(Item::Node(inputs)), Some(Item::Node(op)), Some(Item::Node(outputs))) => {
                        stack.push(Item::Box {
                            inputs,
                            op,
                            outputs,
                        })
                    }
                    _ => notes.push("LD: kutu düğümleri beklenen biçimde değil".into()),
                }
            }
            6 | 29 => {
                let count = node.branches.max(1);
                let mut branches = vec![Vec::new(); count];
                for b in (0..count).rev() {
                    let mut filled = 0;
                    while filled < node.cells {
                        let Some(item) = stack.pop() else { break };
                        filled += width(&item);
                        branches[b].insert(0, item);
                    }
                    if filled != node.cells {
                        notes.push(format!(
                            "LD: paralel dal genişliği {filled}, beklenen {}",
                            node.cells
                        ));
                    }
                }
                stack.push(Item::Par {
                    kind: node.kind,
                    cells: node.cells,
                    branches,
                });
            }
            _ => stack.push(Item::Node(node.clone())),
        }
    }
    stack
}

fn real(series: &[Item]) -> Vec<&Item> {
    series
        .iter()
        .filter(|i| !matches!(i, Item::Node(n) if n.kind == FILLER))
        .collect()
}

fn is_simple(item: &Item) -> bool {
    match item {
        Item::Box { op, .. } => op.kind == 10,
        Item::Node(n) => contact_forms(n.kind).is_some(),
        Item::Par { .. } => false,
    }
}

fn box_text(inputs: &Node, op: &Node, outputs: &Node) -> String {
    let mut text = op.symbol.clone();
    for operand in inputs.vars.iter().chain(&outputs.vars) {
        text.push(' ');
        text.push_str(operand);
    }
    text
}

/// form: 0 = LD, 1 = AND, 2 = OR.
fn contact_line(item: &Item, form: usize) -> String {
    match item {
        Item::Box {
            inputs,
            op,
            outputs,
        } => {
            format!(
                "{}{}",
                ["LD", "AND", "OR"][form],
                box_text(inputs, op, outputs)
            )
        }
        Item::Node(n) => format!(
            "{} {}",
            contact_forms(n.kind).map_or("LD", |f| f[form]),
            n.device
        ),
        Item::Par { .. } => String::new(),
    }
}

fn emit_condition(series: &[&Item], il: &mut Vec<String>, notes: &mut Vec<String>, first: bool) {
    let mut loaded = !first;
    for item in series {
        if is_simple(item) {
            il.push(contact_line(item, usize::from(loaded)));
            loaded = true;
        } else {
            match item {
                Item::Par { branches, .. } => {
                    emit_parallel(branches, il, notes);
                    if loaded {
                        il.push("ANB".into());
                    }
                    loaded = true;
                }
                Item::Node(n) if standalone(n.kind).is_some() => {
                    il.push(standalone(n.kind).unwrap().into())
                }
                Item::Box {
                    inputs,
                    op,
                    outputs,
                } => il.push(box_text(inputs, op, outputs)),
                Item::Node(n) => {
                    notes.push(format!("LD: koşulda çevrilemeyen düğüm türü {}", n.kind));
                    il.push(format!("; ?? LD düğüm türü {}", n.kind));
                }
            }
        }
    }
}

fn emit_parallel(branches: &[Vec<Item>], il: &mut Vec<String>, notes: &mut Vec<String>) {
    for (index, branch) in branches.iter().enumerate() {
        let items = real(branch);
        if index == 0 {
            emit_condition(&items, il, notes, true);
        } else if items.len() == 1 && is_simple(items[0]) {
            il.push(contact_line(items[0], 2));
        } else {
            emit_condition(&items, il, notes, true);
            il.push("ORB".into());
        }
    }
}

fn has_condition(series: &[&Item]) -> bool {
    series.iter().any(|item| {
        is_simple(item)
            || matches!(item, Item::Par { kind: 6, .. })
            || matches!(item, Item::Node(n) if standalone(n.kind).is_some())
    })
}

fn emit_output(series: &[&Item], il: &mut Vec<String>, notes: &mut Vec<String>) {
    for item in series {
        match item {
            Item::Node(n) if coil(n.kind).is_some() => {
                il.push(format!("{} {}", coil(n.kind).unwrap(), n.device))
            }
            Item::Box { op, .. } if op.kind == 10 => il.push(contact_line(item, 1)),
            Item::Box {
                inputs,
                op,
                outputs,
            } => il.push(box_text(inputs, op, outputs)),
            _ if is_simple(item) => il.push(contact_line(item, 1)),
            Item::Node(n) if standalone(n.kind).is_some() => {
                il.push(standalone(n.kind).unwrap().into())
            }
            Item::Par {
                kind: 6, branches, ..
            } => {
                emit_parallel(branches, il, notes);
                il.push("ANB".into());
            }
            Item::Par { branches, .. } => {
                let branches: Vec<Vec<&Item>> = branches.iter().map(|b| real(b)).collect();
                let stacked = branches.iter().any(|b| has_condition(b));
                let last = branches.len().saturating_sub(1);
                for (index, branch) in branches.iter().enumerate() {
                    if stacked {
                        il.push(
                            match index {
                                0 => "MPS",
                                i if i == last => "MPP",
                                _ => "MRD",
                            }
                            .into(),
                        );
                    }
                    emit_output(branch, il, notes);
                }
            }
            Item::Node(n) => {
                notes.push(format!("LD: çıkışta çevrilemeyen düğüm türü {}", n.kind));
                il.push(format!("; ?? LD düğüm türü {}", n.kind));
            }
        }
    }
}

fn network_il(net: &Network, notes: &mut Vec<String>) -> Vec<String> {
    let mut il = Vec::new();
    let root = build_tree(&net.root, notes);
    let out = build_tree(&net.out, notes);
    let root = real(&root);
    let out = real(&out);
    if !root.is_empty() {
        emit_condition(&root, &mut il, notes, true);
    }
    if !out.is_empty() {
        emit_output(&out, &mut il, notes);
    }
    il
}

fn comment_lines(text: &str, out: &mut String) {
    for line in text.lines() {
        out.push_str(&format!("; {}\n", line.trim()).replace("; \n", ";\n"));
    }
}

/// Delta editor IL comments are `(* ... *)`; OpenDVP IL uses `;`.
fn il_body(code: &str, out: &mut String) {
    let mut in_comment = false;
    for line in code.lines() {
        let trimmed = line.trim();
        if in_comment {
            let (text, closed) = match trimmed.split_once("*)") {
                Some((text, _)) => (text, true),
                None => (trimmed, false),
            };
            comment_lines(text, out);
            in_comment = !closed;
        } else if let Some(rest) = trimmed.strip_prefix("(*") {
            match rest.split_once("*)") {
                Some((text, _)) => comment_lines(text, out),
                None => {
                    comment_lines(rest, out);
                    in_comment = true;
                }
            }
        } else if let Some((code, text)) = trimmed.split_once("(*") {
            out.push_str(&format!(
                "{} ; {}\n",
                code.trim_end(),
                text.trim_end_matches("*)").trim()
            ));
        } else if !trimmed.eq_ignore_ascii_case("END") {
            out.push_str(trimmed);
            out.push('\n');
        }
    }
}

/// Delta editor writes IEC literals (`150`, `16#FF`, `1.5`); DVP IL needs `K150`, `HFF`, `F1.5`.
fn dvp_literal(operand: &str) -> Option<String> {
    let digits = operand.strip_prefix('-').unwrap_or(operand);
    if let Some(hex) = operand.strip_prefix("16#") {
        return (!hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| format!("H{}", hex.to_ascii_uppercase()));
    }
    if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        return Some(format!("K{operand}"));
    }
    let mut parts = digits.splitn(2, '.');
    let (whole, fraction) = (parts.next()?, parts.next()?);
    (!whole.is_empty()
        && whole.chars().all(|c| c.is_ascii_digit())
        && !fraction.is_empty()
        && fraction.chars().all(|c| c.is_ascii_digit()))
    .then(|| format!("F{operand}"))
}

/// Replace declared symbols by their device address where the address is concrete, and
/// IEC literals by DVP constants.
fn resolve_symbols(line: &str, symbols: &[(String, String)]) -> String {
    if line.starts_with(';') || line.trim().is_empty() {
        return line.to_owned();
    }
    let (code, rest) = match line.split_once(';') {
        Some((code, rest)) => (code, Some(rest)),
        None => (line, None),
    };
    let mut parts = code.split_whitespace();
    let mut out = parts.next().unwrap_or_default().to_owned();
    for operand in parts {
        out.push(' ');
        match symbols.iter().find(|(name, _)| name == operand) {
            Some((_, address)) => out.push_str(address),
            None => match dvp_literal(operand) {
                Some(literal) => out.push_str(&literal),
                None => out.push_str(operand),
            },
        }
    }
    if let Some(rest) = rest {
        out.push_str(" ;");
        out.push_str(rest);
    }
    out
}

fn pou_order(project: &ProjectText) -> Vec<&Pou> {
    let mut ordered: Vec<&Pou> = project
        .task_order
        .iter()
        .filter_map(|name| project.pous.iter().find(|p| &p.name == name))
        .collect();
    for pou in &project.pous {
        if !ordered.iter().any(|p| p.name == pou.name) {
            ordered.push(pou);
        }
    }
    ordered
}

pub fn import(data: &[u8]) -> Result<Imported, String> {
    let project = parse_project(&project_text(data)?);
    if project.pous.is_empty() {
        return Err("Delta editor projesinde POU bulunamadı".into());
    }
    let mut notes = Vec::new();
    let order = pou_order(&project);
    let convertible: Vec<&Pou> = order
        .iter()
        .copied()
        .filter(|p| p.lang == 0 || p.lang == 1)
        .collect();
    let lang_name = |lang: i32| match lang {
        0 => "IL",
        1 => "LD",
        2 => "FBD",
        3 => "SFC",
        4 => "ST",
        _ => "?",
    };

    if convertible.is_empty() {
        // ST only: wrap the first ST POU as an OpenDVP Structured Text program.
        let pou = order
            .iter()
            .find(|p| p.lang == 4 && p.code.is_some())
            .ok_or("Delta editor projesinde IL, LD veya ST POU yok (FBD/SFC çevrilmiyor)")?;
        let mut source = format!("PROGRAM {}\nVAR\n", pou.name);
        for var in pou.vars.iter().chain(&project.globals) {
            match &var.address {
                Some(address) => {
                    source.push_str(&format!("    {} AT {address} : {};\n", var.name, var.kind))
                }
                None => source.push_str(&format!("    {} : {};\n", var.name, var.kind)),
            }
        }
        source.push_str("END_VAR\n\n");
        source.push_str(pou.code.as_deref().unwrap_or_default());
        source.push_str("\nEND_PROGRAM\n");
        for other in order.iter().filter(|p| p.name != pou.name) {
            notes.push(format!(
                "{} ({}) içe aktarılmadı",
                other.name,
                lang_name(other.lang)
            ));
        }
        return Ok(Imported {
            language: crate::project::SourceLanguage::St,
            source,
            notes,
            device_comments: project.device_comments.clone(),
        });
    }

    let mut source = String::new();
    for pou in &order {
        if pou.lang != 0 && pou.lang != 1 {
            notes.push(format!(
                "{} ({}) içe aktarılmadı",
                pou.name,
                lang_name(pou.lang)
            ));
            continue;
        }
        let symbols: Vec<(String, String)> = pou
            .vars
            .iter()
            .chain(&project.globals)
            .filter_map(|v| Some((v.name.clone(), v.address.clone()?)))
            .collect();
        let mut body = String::new();
        if pou.lang == 0 {
            il_body(pou.code.as_deref().unwrap_or_default(), &mut body);
        } else {
            for net in &pou.networks {
                body.push('\n');
                comment_lines(&net.comment, &mut body);
                for line in network_il(net, &mut notes) {
                    body.push_str(&line);
                    body.push('\n');
                }
            }
        }
        source.push_str(&format!(
            "\n; ===== POU {} ({}) =====\n",
            pou.name,
            lang_name(pou.lang)
        ));
        if !pou.comment.is_empty() {
            comment_lines(&pou.comment, &mut source);
        }
        for line in body.lines() {
            source.push_str(&resolve_symbols(line, &symbols));
            source.push('\n');
        }
    }
    source.push_str("END\n");
    Ok(Imported {
        language: crate::project::SourceLanguage::Il,
        source: source.trim_start().to_owned(),
        notes,
        device_comments: project.device_comments.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLES: &str = "analysis/isp-dvp-formats/samples";

    fn sample(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/{SAMPLES}/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn instructions(il: &str) -> Vec<String> {
        il.lines()
            .map(|l| l.split(';').next().unwrap().trim())
            .filter(|l| !l.is_empty() && !l.starts_with('='))
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect()
    }

    #[test]
    fn iec_literals_become_dvp_constants() {
        let line = resolve_symbols("TMR T0 150", &[]);
        assert_eq!(line, "TMR T0 K150");
        assert_eq!(resolve_symbols("MOV 16#1f D0", &[]), "MOV H1F D0");
        assert_eq!(resolve_symbols("DEMOV 1.5 D0", &[]), "DEMOV F1.5 D0");
        assert_eq!(
            resolve_symbols("MOV -3 D0 ; note", &[]),
            "MOV K-3 D0 ; note"
        );
        let symbols = [("ACT".to_string(), "D0".to_string())];
        assert_eq!(resolve_symbols("LD= ACT 0", &symbols), "LD= D0 K0");
        // Device names are left alone.
        assert_eq!(resolve_symbols("OUT Y10", &[]), "OUT Y10");
    }

    #[test]
    fn every_sample_project_text_unpacks_with_matching_crc() {
        skip_without_fixtures!("isp-dvp-formats/samples/Signal.isp");
        for name in [
            "Signal.isp",
            "Edge_Detection_in_ST.isp",
            "Merry_Xmas_and_Happy_New_Year.isp",
            "Signal_Control.isp",
            "Tray_Transfer.isp",
            "Gluing_System_C/Gluing_System_C.isp",
        ] {
            let text = project_text(&sample(name)).unwrap();
            assert!(
                text.contains("<PROJECT>") && text.contains("<POU>"),
                "{name}"
            );
        }
    }

    /// Signal.isp holds the same program as LD (Signal_LD) and hand-written IL (Signal_IL).
    #[test]
    fn ld_conversion_matches_the_hand_written_il_version() {
        skip_without_fixtures!("isp-dvp-formats/samples/Signal.isp");
        let project = parse_project(&project_text(&sample("Signal.isp")).unwrap());
        let pou = |name: &str| project.pous.iter().find(|p| p.name == name).unwrap();
        let mut notes = Vec::new();
        let mut ld = String::new();
        for net in &pou("Signal_LD").networks {
            for line in network_il(net, &mut notes) {
                ld.push_str(&line);
                ld.push('\n');
            }
        }
        let mut il = String::new();
        il_body(pou("Signal_IL").code.as_deref().unwrap(), &mut il);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(instructions(&ld), instructions(&il));
        assert_eq!(instructions(&ld).len(), 23);
    }

    #[test]
    fn imports_ld_project_and_st_only_project() {
        skip_without_fixtures!("isp-dvp-formats/samples/Signal.isp");
        let gluing = import(&sample("Gluing_System_C/Gluing_System_C.isp")).unwrap();
        assert_eq!(gluing.language, crate::project::SourceLanguage::Il);
        assert!(gluing.source.trim_end().ends_with("END"));
        assert!(gluing.source.contains("LDP") && gluing.source.contains("AND>="));
        let st = import(&sample("Edge_Detection_in_ST.isp")).unwrap();
        assert_eq!(st.language, crate::project::SourceLanguage::St);
        assert!(st.source.starts_with("PROGRAM "));
        assert!(import(b"not an isp file").is_err());
    }
}
