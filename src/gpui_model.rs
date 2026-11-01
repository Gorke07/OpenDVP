//! Framework-independent graphical edits. Commit only after compiling the candidate.
use crate::ladder_source::emit_expr;
use opendvp::{
    tfmt,
    i18n::tr,
    plc::{self, Action, Device, Expr},
    project::SourceLanguage,
};
use std::{cell::RefCell, rc::Rc};

/// Compiles an edited source text the way the owner of that text compiles it. A program
/// unit of a multi-program project is only valid inside the joined program, so the
/// workbench installs the project's compiler around the edit; without one the text is
/// compiled on its own.
pub type Compiler = Rc<dyn Fn(&str) -> Result<plc::Program, plc::Diagnostic>>;
thread_local! {
    static COMPILER: RefCell<Option<Compiler>> = const { RefCell::new(None) };
}
/// Run `f` with `compiler` validating the edited sources.
pub fn with_compiler<R>(compiler: Compiler, f: impl FnOnce() -> R) -> R {
    let previous = COMPILER.with(|c| c.replace(Some(compiler)));
    let result = f();
    COMPILER.with(|c| *c.borrow_mut() = previous);
    result
}
/// `with_compiler` when there is one, otherwise just `f`.
pub fn scoped<R>(compiler: Option<Compiler>, f: impl FnOnce() -> R) -> R {
    match compiler {
        Some(compiler) => with_compiler(compiler, f),
        None => f(),
    }
}
fn compile(source: &str) -> Result<plc::Program, plc::Diagnostic> {
    let compiler = COMPILER.with(|c| c.borrow().clone());
    match compiler {
        Some(compile) => compile(source),
        None => plc::compile(source),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Condition(usize),
    Action(usize),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Before,
    After,
    Parallel,
}
#[derive(Clone)]
pub enum ElementEdit {
    Replace(String),
    InsertContact(Device, bool, Placement),
    /// Any single condition line: edge contacts (`LDP X0`), compare contacts (`LD= D0 K10`).
    InsertCondition(String, Placement),
    InsertAction(String, Placement),
    Delete,
}

fn has_saved(e: &Expr) -> bool {
    match e {
        Expr::Saved(_) => true,
        Expr::And(a, b) | Expr::Or(a, b) => has_saved(a) || has_saved(b),
        Expr::Trigger(inner, ..) | Expr::Invert(inner) => has_saved(inner),
        _ => false,
    }
}
fn leftmost(e: &Expr) -> &Expr {
    match e {
        Expr::And(a, _) | Expr::Or(a, _) => leftmost(a),
        _ => e,
    }
}
/// A branch condition (after MPS/MRD/MPP) can be re-emitted as IL only while the saved
/// value stays its leftmost operand and appears once: the IL then continues from it.
fn check_branch_condition(original: &Expr, edited: &Expr) -> Result<(), String> {
    fn count(e: &Expr) -> usize {
        match e {
            Expr::Saved(_) => 1,
            Expr::And(a, b) | Expr::Or(a, b) => count(a) + count(b),
            Expr::Trigger(inner, ..) | Expr::Invert(inner) => count(inner),
            _ => 0,
        }
    }
    if !matches!(leftmost(original), Expr::Saved(_)) || count(original) != 1 {
        return Err(tr("Bu dal kaynak editöründe düzenlenmeli.").into());
    }
    if leftmost(edited) != leftmost(original) || count(edited) != 1 {
        return Err(
            tr("Dalın ortak kısmı (KAYITLI) silinemez, değiştirilemez ve önüne kontak eklenemez;              ortak kısmı kaynak editöründe düzenleyin.")
                .into(),
        );
    }
    Ok(())
}
/// Recompile and make sure the edited branch produces exactly the intended condition.
fn check_branch_result(
    compiled: &plc::Program,
    network: usize,
    wanted: &Expr,
) -> Result<(), String> {
    match compiled.networks.get(network) {
        Some(n) if &n.condition == wanted => Ok(()),
        _ => Err(tr("Dal düzenlemesi IL'de aynı mantığı üretmedi; değişiklik uygulanmadı.").into()),
    }
}
pub fn condition_at(expr: &Expr, index: usize) -> Option<&Expr> {
    fn visit<'a>(e: &'a Expr, wanted: usize, n: &mut usize) -> Option<&'a Expr> {
        match e {
            Expr::And(a, b) | Expr::Or(a, b) => visit(a, wanted, n).or_else(|| visit(b, wanted, n)),
            _ => {
                let current = *n;
                *n += 1;
                (current == wanted).then_some(e)
            }
        }
    }
    visit(expr, index, &mut 0)
}
fn single_line(text: &str) -> Result<&str, String> {
    let line = text.trim();
    if line.is_empty() || line.contains(['\n', '\r']) {
        Err(tr("Tek bir komut satırı girin.").into())
    } else {
        Ok(line)
    }
}
fn action_from(text: &str) -> Result<Action, String> {
    let p = plc::compile(&format!("LD M0\n{}\nEND\n", single_line(text)?))
        .map_err(|e| e.to_string())?;
    if p.requires_source_editing() || p.networks.len() != 1 || p.networks[0].actions.len() != 1 {
        return Err(tr("Tek bir çıkış veya fonksiyon komutu girin.").into());
    }
    let action = p.networks[0].actions[0].clone();
    if matches!(&action,Action::Out(d) if !d.writable_bit()) {
        return Err(tr("Bobin Y, M veya S olmalı.").into());
    }
    Ok(action)
}
fn condition_from(text: &str) -> Result<Expr, String> {
    let p = plc::compile(&format!("{}\nOUT Y0\nEND\n", single_line(text)?))
        .map_err(|e| e.to_string())?;
    if p.requires_source_editing() || p.networks.len() != 1 || p.networks[0].actions.len() != 1 {
        return Err(tr("LD / LDI veya tek bir koşul komutu girin.").into());
    }
    Ok(p.networks[0].condition.clone())
}
pub fn edit_element(
    source: &str,
    language: SourceLanguage,
    network: usize,
    target: Target,
    op: ElementEdit,
) -> Result<String, String> {
    if language != SourceLanguage::Il {
        return Err(tr("ST kaynağı grafik olarak salt okunur.").into());
    }
    let p = compile(source).map_err(|e| e.to_string())?;
    if !p.graphically_editable() {
        return Err(tr("Akış kontrollü programı kaynak editöründe düzenleyin.").into());
    }
    let net = p
        .networks
        .get(network)
        .ok_or(tr("Ağ artık mevcut değil; yeniden seçin."))?;
    let mut condition = net.condition.clone();
    let mut actions = net.actions.clone();
    if let Target::Condition(wanted) = target {
        if condition_at(&condition, wanted).is_none() {
            return Err(tr("Eleman artık mevcut değil; yeniden seçin.").into());
        }
        let replacement = match &op {
            ElementEdit::Replace(text) => Some(condition_from(text)?),
            ElementEdit::InsertContact(d, nc, _) => {
                if !d.bit() {
                    return Err(tr("Kontak için bit cihazı girin.").into());
                }
                Some(Expr::Contact(*d, *nc))
            }
            ElementEdit::InsertCondition(text, _) => Some(condition_from(text)?),
            _ => None,
        };
        fn rewrite(
            e: Expr,
            wanted: usize,
            n: &mut usize,
            op: &ElementEdit,
            replacement: &Option<Expr>,
        ) -> Option<Expr> {
            match e {
                Expr::And(a, b) => {
                    let a = rewrite(*a, wanted, n, op, replacement);
                    let b = rewrite(*b, wanted, n, op, replacement);
                    match (a, b) {
                        (Some(a), Some(b)) => Some(Expr::And(Box::new(a), Box::new(b))),
                        (a, b) => a.or(b),
                    }
                }
                Expr::Or(a, b) => {
                    let a = rewrite(*a, wanted, n, op, replacement);
                    let b = rewrite(*b, wanted, n, op, replacement);
                    match (a, b) {
                        (Some(a), Some(b)) => Some(Expr::Or(Box::new(a), Box::new(b))),
                        (a, b) => a.or(b),
                    }
                }
                leaf => {
                    let index = *n;
                    *n += 1;
                    if index != wanted {
                        return Some(leaf);
                    }
                    match op {
                        ElementEdit::Delete => None,
                        ElementEdit::Replace(_) => replacement.clone(),
                        ElementEdit::InsertContact(_, _, placement)
                        | ElementEdit::InsertCondition(_, placement) => {
                            let new = replacement.clone()?;
                            Some(match placement {
                                Placement::Before => Expr::And(Box::new(new), Box::new(leaf)),
                                Placement::After => Expr::And(Box::new(leaf), Box::new(new)),
                                Placement::Parallel => Expr::Or(Box::new(leaf), Box::new(new)),
                            })
                        }
                        _ => Some(leaf),
                    }
                }
            }
        }
        condition = rewrite(condition, wanted, &mut 0, &op, &replacement)
            .ok_or(tr("Ağın son koşulu silinemez; ağı kaldırın veya koşulu değiştirin."))?;
        if let ElementEdit::InsertAction(text, _) = &op {
            actions.push(action_from(text)?);
        }
    } else if let Target::Action(index) = target {
        if index >= actions.len() {
            return Err(tr("Komut artık mevcut değil; yeniden seçin.").into());
        }
        match &op {
            ElementEdit::Replace(text) => actions[index] = action_from(text)?,
            ElementEdit::InsertAction(text, place) => actions.insert(
                if *place == Placement::Before {
                    index
                } else {
                    index + 1
                },
                action_from(text)?,
            ),
            ElementEdit::Delete => {
                actions.remove(index);
                if actions.is_empty() {
                    return Err(
                        tr("Son çıkış komutu silinemez; ağı kaldırın veya komutu değiştirin.").into(),
                    );
                }
            }
            ElementEdit::InsertContact(d, nc, _) => {
                if !d.bit() {
                    return Err(tr("Kontak için bit cihazı girin.").into());
                }
                condition = Expr::And(Box::new(condition), Box::new(Expr::Contact(*d, *nc)));
            }
            ElementEdit::InsertCondition(text, _) => {
                condition = Expr::And(Box::new(condition), Box::new(condition_from(text)?));
            }
        }
    }
    let branch = has_saved(&net.condition);
    if branch {
        check_branch_condition(&net.condition, &condition)?;
    }
    let mut replacement = String::new();
    emit_expr(&condition, &mut replacement);
    for action in actions {
        replacement.push_str(&action.label());
        replacement.push('\n');
    }
    let mut lines = source.lines().map(str::to_owned).collect::<Vec<_>>();
    lines.splice(
        net.line - 1..net.end_line,
        replacement.lines().map(str::to_owned),
    );
    let candidate = lines.join("\n") + "\n";
    let compiled = compile(&candidate).map_err(|e| e.to_string())?;
    if !compiled.graphically_editable() {
        return Err(tr("Bu komut kaynak editöründe düzenlenmeli.").into());
    }
    if branch {
        check_branch_result(&compiled, network, &condition)?;
    }
    Ok(candidate)
}

#[derive(Clone, Copy)]
pub enum Edit {
    SeriesNo(Device),
    SeriesNc(Device),
    Parallel(Device),
    Output(Device),
    Add,
    Remove,
}

pub fn edit(
    source: &str,
    language: SourceLanguage,
    index: usize,
    edit: Edit,
) -> Result<String, String> {
    if language != SourceLanguage::Il {
        return Err(tr("ST kaynağını Kaynak editöründe düzenleyin.").into());
    }
    let program = compile(source).map_err(|e| e.to_string())?;
    if !program.graphically_editable() {
        return Err(tr("Akış kontrolü içeren programı Kaynak editöründe düzenleyin.").into());
    }
    let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
    let mut branch_check = None;
    if matches!(edit, Edit::Add) {
        let output = (0..256)
            .map(|index| Device {
                area: plc::Area::Y,
                index,
            })
            .find(|d| !program.devices.contains(d))
            .ok_or(tr("Boş Y çıkışı bulunamadı."))?;
        let end = lines
            .iter()
            .position(|s| {
                s.split(';')
                    .next()
                    .unwrap_or_default()
                    .split("//")
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .eq_ignore_ascii_case("END")
            })
            .ok_or(tr("END bulunamadı."))?;
        lines.splice(
            end..end,
            [tr("; Yeni ağ").into(), "LD X0".into(), format!("OUT {output}")],
        );
    } else {
        let network = program.networks.get(index).ok_or(tr("Önce bir ağ seçin."))?;
        let branch = has_saved(&network.condition);
        if branch && matches!(edit, Edit::Remove) {
            return Err(
                tr("MPS/MRD/MPP dalı tek başına kaldırılamaz; dallanmayı kaynak editöründe düzenleyin.")
                    .into(),
            );
        }
        let mut replacement = String::new();
        if !matches!(edit, Edit::Remove) {
            let condition = match edit {
                Edit::SeriesNo(d) | Edit::SeriesNc(d) | Edit::Parallel(d) => {
                    if !d.bit() {
                        return Err(tr("Kontak için bit cihazı girin.").into());
                    }
                    if matches!(edit, Edit::Parallel(_)) {
                        Expr::Or(
                            Box::new(network.condition.clone()),
                            Box::new(Expr::Contact(d, false)),
                        )
                    } else {
                        Expr::And(
                            Box::new(network.condition.clone()),
                            Box::new(Expr::Contact(d, matches!(edit, Edit::SeriesNc(_)))),
                        )
                    }
                }
                _ => network.condition.clone(),
            };
            if branch {
                check_branch_condition(&network.condition, &condition)?;
                branch_check = Some(condition.clone());
            }
            emit_expr(&condition, &mut replacement);
            let mut replaced = false;
            for action in &network.actions {
                if let Edit::Output(d) = edit
                    && !replaced
                    && matches!(action, Action::Out(_))
                {
                    if !d.writable_bit() {
                        return Err(tr("Bobin Y, M veya S olmalı.").into());
                    }
                    replacement.push_str(&format!("OUT {d}\n"));
                    replaced = true;
                } else {
                    replacement.push_str(&action.label());
                    replacement.push('\n');
                }
            }
            if let Edit::Output(d) = edit
                && !replaced
            {
                if !d.writable_bit() {
                    return Err(tr("Bobin Y, M veya S olmalı.").into());
                }
                replacement.push_str(&format!("OUT {d}\n"));
            }
        }
        lines.splice(
            network.line - 1..network.end_line,
            replacement.lines().map(str::to_owned),
        );
    }
    let candidate = lines.join("\n") + "\n";
    let compiled = compile(&candidate).map_err(|e| e.to_string())?;
    if let Some(condition) = branch_check {
        check_branch_result(&compiled, index, &condition)?;
    }
    Ok(candidate)
}

/// Source lines (0-based, end exclusive) of network `index` including the comment block
/// directly above it. MPS/MRD/MPP branch networks share state with their neighbours and
/// cannot be moved alone.
fn network_block(
    program: &plc::Program,
    source: &str,
    index: usize,
) -> Result<(usize, usize), String> {
    let network = program.networks.get(index).ok_or(tr("Önce bir ağ seçin."))?;
    if has_saved(&network.condition) {
        return Err(tr("MPS/MRD/MPP dalı tek başına kopyalanamaz veya taşınamaz.").into());
    }
    let lines: Vec<&str> = source.lines().collect();
    let mut start = network.line - 1;
    while start > 0 {
        let line = lines[start - 1].trim();
        if line.starts_with(';') || line.starts_with("//") {
            start -= 1;
        } else {
            break;
        }
    }
    Ok((start, network.end_line))
}

fn editable_program(source: &str, language: SourceLanguage) -> Result<plc::Program, String> {
    if language != SourceLanguage::Il {
        return Err(tr("ST kaynağını Kaynak editöründe düzenleyin.").into());
    }
    let program = compile(source).map_err(|e| e.to_string())?;
    if !program.graphically_editable() {
        return Err(tr("Akış kontrolü içeren programı Kaynak editöründe düzenleyin.").into());
    }
    Ok(program)
}

/// IL text of network `index` with its comment, for the clipboard.
pub fn copy_network(
    source: &str,
    language: SourceLanguage,
    index: usize,
) -> Result<String, String> {
    let program = editable_program(source, language)?;
    let (start, end) = network_block(&program, source, index)?;
    Ok(source.lines().collect::<Vec<_>>()[start..end].join("\n") + "\n")
}

/// Insert IL networks after network `after` (or before END when `None`). The text must
/// compile on its own; the result must gain exactly its networks.
pub fn paste_networks(
    source: &str,
    language: SourceLanguage,
    after: Option<usize>,
    text: &str,
) -> Result<(String, usize), String> {
    let program = editable_program(source, language)?;
    let body: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().eq_ignore_ascii_case("END"))
        .collect();
    let pasted = plc::compile(&(body.join("\n") + "\nEND\n"))
        .map_err(|e| tfmt!("Panodaki metin geçerli IL ağı değil: {e}", "The clipboard text is not a valid IL network: {e}"))?;
    if pasted.networks.is_empty() {
        return Err(tr("Panoda yapıştırılacak ağ yok.").into());
    }
    let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
    let at = match after {
        Some(index) => network_block(&program, source, index)?.1,
        None => lines
            .iter()
            .position(|l| l.trim().eq_ignore_ascii_case("END"))
            .ok_or(tr("END bulunamadı."))?,
    };
    let mut insert = vec![String::new()];
    insert.extend(body.iter().map(|l| l.to_string()));
    lines.splice(at..at, insert);
    let candidate = lines.join("\n") + "\n";
    let compiled = compile(&candidate).map_err(|e| e.to_string())?;
    if compiled.networks.len() != program.networks.len() + pasted.networks.len() {
        return Err(tr("Yapıştırma ağ yapısını bozdu; değişiklik uygulanmadı.").into());
    }
    Ok((candidate, pasted.networks.len()))
}

/// Swap network `index` with its neighbour above (`up`) or below.
pub fn move_network(
    source: &str,
    language: SourceLanguage,
    index: usize,
    up: bool,
) -> Result<String, String> {
    let program = editable_program(source, language)?;
    let other = if up {
        index.checked_sub(1).ok_or(tr("Ağ zaten en üstte."))?
    } else {
        Some(index + 1)
            .filter(|&i| i < program.networks.len())
            .ok_or(tr("Ağ zaten en altta."))?
    };
    let (first, second) = (index.min(other), index.max(other));
    let (a_start, a_end) = network_block(&program, source, first)?;
    let (b_start, b_end) = network_block(&program, source, second)?;
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<&str> = lines[..a_start].to_vec();
    out.extend(&lines[b_start..b_end]);
    out.extend(&lines[a_end..b_start]);
    out.extend(&lines[a_start..a_end]);
    out.extend(&lines[b_end..]);
    let candidate = out.join("\n") + "\n";
    let compiled = compile(&candidate).map_err(|e| e.to_string())?;
    if compiled.networks.len() != program.networks.len() {
        return Err(tr("Taşıma ağ yapısını bozdu; değişiklik uygulanmadı.").into());
    }
    Ok(candidate)
}

/// Replace the comment block directly above network `index` (first line = title, the rest
/// = description). An empty comment removes the block.
pub fn set_network_comment(
    source: &str,
    language: SourceLanguage,
    index: usize,
    comment: &str,
) -> Result<String, String> {
    if language != SourceLanguage::Il {
        return Err(tr("ST kaynağını Kaynak editöründe düzenleyin.").into());
    }
    let program = compile(source).map_err(|e| e.to_string())?;
    if !program.graphically_editable() {
        return Err(tr("Akış kontrolü içeren programı Kaynak editöründe düzenleyin.").into());
    }
    let network = program.networks.get(index).ok_or(tr("Önce bir ağ seçin."))?;
    let wanted: Vec<String> = comment
        .lines()
        .map(|l| l.trim().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_matches('\n')
        .lines()
        .map(str::to_owned)
        .collect();
    let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
    let first = network.line - 1;
    let mut start = first;
    while start > 0 {
        let line = lines[start - 1].trim();
        if line.starts_with(';') || line.starts_with("//") {
            start -= 1;
        } else {
            break;
        }
    }
    lines.splice(start..first, wanted.iter().map(|l| format!("; {l}")));
    let candidate = lines.join("\n") + "\n";
    let compiled = compile(&candidate).map_err(|e| e.to_string())?;
    let expected = wanted.join("\n");
    match compiled.networks.get(index) {
        Some(n) if compiled.networks.len() == program.networks.len() && n.comment == expected => {
            Ok(candidate)
        }
        _ => Err(tr("Ağ açıklaması bu konuma yazılamadı; Kaynak editöründe düzenleyin.").into()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn networks_copy_paste_and_move_with_their_comments() {
        let source = "; Bir\nLD X0\nOUT Y0\n\n; İki\nLD X1\nOUT Y1\nEND\n";
        let copied = copy_network(source, SourceLanguage::Il, 0).unwrap();
        assert_eq!(copied, "; Bir\nLD X0\nOUT Y0\n");
        let (pasted, count) = paste_networks(source, SourceLanguage::Il, Some(1), &copied).unwrap();
        assert_eq!(count, 1);
        let p = plc::compile(&pasted).unwrap();
        assert_eq!(p.networks.len(), 3);
        assert_eq!(p.networks[2].comment, "Bir");
        let moved = move_network(source, SourceLanguage::Il, 1, true).unwrap();
        let p = plc::compile(&moved).unwrap();
        assert_eq!(p.networks[0].comment, "İki");
        assert_eq!(p.networks[1].comment, "Bir");
        assert!(move_network(source, SourceLanguage::Il, 0, true).is_err());
        assert!(
            paste_networks(source, SourceLanguage::Il, None, "bu IL değil")
                .unwrap_err()
                .contains("geçerli IL")
        );
        // Branch networks depend on the shared MPS value.
        assert!(copy_network(MPS_SOURCE, SourceLanguage::Il, 1).is_err());
    }
    #[test]
    fn palette_conditions_insert_edge_and_compare_contacts() {
        let source = "LD X0\nOUT Y0\nEND\n";
        let edge = edit_element(
            source,
            SourceLanguage::Il,
            0,
            Target::Condition(0),
            ElementEdit::InsertCondition("LDP X1".into(), Placement::After),
        )
        .unwrap();
        assert!(edge.contains("X1"), "{edge}");
        let compare = edit_element(
            &edge,
            SourceLanguage::Il,
            0,
            Target::Action(0),
            ElementEdit::InsertCondition("LD>= D0 K10".into(), Placement::After),
        )
        .unwrap();
        assert!(compare.contains(">="), "{compare}");
        let p = plc::compile(&compare).unwrap();
        assert_eq!(p.networks.len(), 1);
        assert!(
            edit_element(
                source,
                SourceLanguage::Il,
                0,
                Target::Condition(0),
                ElementEdit::InsertCondition("MOV K1 D0".into(), Placement::After),
            )
            .is_err()
        );
    }
    #[test]
    fn network_comment_is_replaced_in_place_and_keeps_other_networks() {
        let source = "; Eski başlık\nLD X0\nOUT Y0\n\n; İkinci\n; açıklama\nLD X1\nOUT Y1\nEND\n";
        let edited = set_network_comment(
            source,
            SourceLanguage::Il,
            1,
            "Pompa kontrolü\nSeviye düşükse çalışır",
        )
        .unwrap();
        let p = plc::compile(&edited).unwrap();
        assert_eq!(p.networks[0].comment, "Eski başlık");
        assert_eq!(
            p.networks[1].comment,
            "Pompa kontrolü\nSeviye düşükse çalışır"
        );
        // Removing the comment deletes the block.
        let removed = set_network_comment(&edited, SourceLanguage::Il, 0, "").unwrap();
        assert!(!removed.contains("Eski başlık"));
        assert_eq!(plc::compile(&removed).unwrap().networks.len(), 2);
    }
    use super::*;
    const MPS_SOURCE: &str = "LD X0
MPS
AND X1
OUT Y0
MRD
ANI X2
OUT Y1
MPP
AND X3
OUT Y2
LD X5
OUT Y5
END
";

    #[test]
    fn mps_branches_are_editable_and_keep_the_stack_structure() {
        // Replace X2 (element 1 of branch 1; element 0 is the saved value) with M7.
        let edited = edit_element(
            MPS_SOURCE,
            SourceLanguage::Il,
            1,
            Target::Condition(1),
            ElementEdit::Replace("LDI M7".into()),
        )
        .unwrap();
        assert!(edited.contains("MPS") && edited.contains("MRD") && edited.contains("MPP"));
        let p = plc::compile(&edited).unwrap();
        assert_eq!(p.networks.len(), 4);
        assert!(matches!(
            &p.networks[1].condition,
            Expr::And(a, b) if matches!(**a, Expr::Saved(2))
                && **b == Expr::Contact("M7".parse().unwrap(), true)
        ));
        // Other branches and the following network are untouched.
        let before = plc::compile(MPS_SOURCE).unwrap();
        for i in [0, 2, 3] {
            assert_eq!(p.networks[i].condition, before.networks[i].condition);
            assert_eq!(
                p.networks[i].actions.len(),
                before.networks[i].actions.len()
            );
        }

        // Series contact, parallel contact and a new output on a branch.
        let series = edit(
            MPS_SOURCE,
            SourceLanguage::Il,
            0,
            Edit::SeriesNc("M1".parse().unwrap()),
        )
        .unwrap();
        let parallel = edit(
            &series,
            SourceLanguage::Il,
            2,
            Edit::Parallel("X7".parse().unwrap()),
        )
        .unwrap();
        let output = edit(
            &parallel,
            SourceLanguage::Il,
            1,
            Edit::Output("Y6".parse().unwrap()),
        )
        .unwrap();
        let p = plc::compile(&output).unwrap();
        assert!(p.has_branches());
        assert_eq!(p.networks.len(), 4);
        assert!(output.contains("OUT Y6"));

        // Action edit on a branch.
        let tmr = edit_element(
            MPS_SOURCE,
            SourceLanguage::Il,
            2,
            Target::Action(0),
            ElementEdit::Replace("TMR T3 K20".into()),
        )
        .unwrap();
        assert!(tmr.contains("TMR T3 K20"));
    }

    #[test]
    fn mps_shared_part_and_branch_removal_stay_source_only() {
        for op in [ElementEdit::Delete, ElementEdit::Replace("LD M0".into())] {
            assert!(
                edit_element(MPS_SOURCE, SourceLanguage::Il, 0, Target::Condition(0), op)
                    .unwrap_err()
                    .contains("ortak kısmı")
            );
        }
        let before = edit_element(
            MPS_SOURCE,
            SourceLanguage::Il,
            0,
            Target::Condition(0),
            ElementEdit::InsertContact("M0".parse().unwrap(), false, Placement::Before),
        );
        assert!(before.unwrap_err().contains("ortak kısmı"));
        assert!(
            edit(MPS_SOURCE, SourceLanguage::Il, 1, Edit::Remove)
                .unwrap_err()
                .contains("tek başına")
        );
        // A plain network in the same program is still freely editable.
        assert!(edit(MPS_SOURCE, SourceLanguage::Il, 3, Edit::Remove).is_ok());
    }

    #[test]
    fn selected_branch_replace_and_insert_preserve_other_branches_and_networks() {
        let source = "; latch\nLD X0\nOR Y0\nANI X1\nOUT Y0\n; timer\nLD Y0\nTMR T0 K30\nEND\n";
        let replace = edit_element(
            source,
            SourceLanguage::Il,
            0,
            Target::Condition(1),
            ElementEdit::Replace("LD M2".into()),
        )
        .unwrap();
        let insert = edit_element(
            &replace,
            SourceLanguage::Il,
            0,
            Target::Condition(1),
            ElementEdit::InsertContact("X3".parse().unwrap(), true, Placement::After),
        )
        .unwrap();
        let p = plc::compile(&insert).unwrap();
        let mut sim = plc::Simulator::default();
        sim.memory.set_input("X0".parse().unwrap(), true);
        sim.memory.set_input("X3".parse().unwrap(), true);
        sim.scan(&p, 20);
        assert!(
            sim.memory.bit("Y0".parse().unwrap()),
            "X3 must gate only the M2 branch"
        );
        assert!(insert.ends_with("; timer\nLD Y0\nTMR T0 K30\nEND\n"));
    }
    #[test]
    fn deleting_branch_collapses_only_its_parent_and_last_element_is_rejected() {
        let source = "LD X0\nOR X1\nAND X2\nOUT Y0\nEND\n";
        let candidate = edit_element(
            source,
            SourceLanguage::Il,
            0,
            Target::Condition(1),
            ElementEdit::Delete,
        )
        .unwrap();
        assert!(!candidate.contains("X1"));
        assert!(candidate.contains("X2"));
        assert!(
            edit_element(
                "LD X0\nOUT Y0\nEND\n",
                SourceLanguage::Il,
                0,
                Target::Condition(0),
                ElementEdit::Delete
            )
            .is_err()
        );
        assert!(
            edit_element(
                source,
                SourceLanguage::Il,
                0,
                Target::Action(0),
                ElementEdit::Delete
            )
            .is_err()
        );
    }
    #[test]
    fn action_selection_is_exact_and_rejects_instruction_injection() {
        let s = "LD X0\nOUT Y0\nMOV K5 D0\nEND\n";
        let candidate = edit_element(
            s,
            SourceLanguage::Il,
            0,
            Target::Action(1),
            ElementEdit::Replace("TMR T0 K30".into()),
        )
        .unwrap();
        assert!(candidate.contains("OUT Y0\nTMR T0 K30"));
        assert!(
            edit_element(
                s,
                SourceLanguage::Il,
                0,
                Target::Action(0),
                ElementEdit::Replace("OUT X0".into())
            )
            .is_err()
        );
        assert!(
            edit_element(
                s,
                SourceLanguage::Il,
                0,
                Target::Action(0),
                ElementEdit::Replace("OUT Y1\nOUT Y2".into())
            )
            .is_err()
        );
    }
    #[test]
    fn branch_edit_keeps_latching_logic_and_following_network() {
        let source = "; latch\nLD X0\nOUT Y0\nLD X2\nTMR T0 K5\nEND\n";
        let candidate = edit(
            source,
            SourceLanguage::Il,
            0,
            Edit::Parallel("Y0".parse().unwrap()),
        )
        .unwrap();
        let candidate = edit(
            &candidate,
            SourceLanguage::Il,
            0,
            Edit::SeriesNc("X1".parse().unwrap()),
        )
        .unwrap();
        let p = plc::compile(&candidate).unwrap();
        let mut sim = plc::Simulator::default();
        sim.memory.set_input("X0".parse().unwrap(), true);
        sim.scan(&p, 20);
        sim.memory.set_input("X0".parse().unwrap(), false);
        sim.scan(&p, 20);
        assert!(sim.memory.bit("Y0".parse().unwrap()));
        sim.memory.set_input("X1".parse().unwrap(), true);
        sim.scan(&p, 20);
        assert!(!sim.memory.bit("Y0".parse().unwrap()));
        assert!(candidate.contains("TMR T0 K5"));
        assert!(candidate.starts_with("; latch"));
    }
    #[test]
    fn rejected_edit_never_changes_st_or_flow() {
        assert!(
            edit(
                opendvp::project::ST_TEMPLATE,
                SourceLanguage::St,
                0,
                Edit::Add
            )
            .is_err()
        );
        assert!(
            edit(
                "LD X0\nOUT Y0\nEND\n",
                SourceLanguage::Il,
                0,
                Edit::Output("D0".parse().unwrap())
            )
            .is_err()
        );
    }
    #[test]
    fn add_remove_maintains_end_and_allocates_unused_output() {
        let source = "LD X0\nOUT Y0\nEND\n";
        let add = edit(source, SourceLanguage::Il, 0, Edit::Add).unwrap();
        assert!(add.contains("OUT Y1"));
        let removed = edit(&add, SourceLanguage::Il, 1, Edit::Remove).unwrap();
        assert_eq!(plc::compile(&removed).unwrap().networks.len(), 1);
        assert!(removed.ends_with("END\n"));
    }

    #[test]
    fn unit_edits_are_validated_inside_the_joined_program() {
        use opendvp::project::{Project, UnitKind};
        let mut project = Project {
            source: "LD X0
CALL SR_1
END
".into(),
            ..Project::default()
        };
        let index = project.add_unit(UnitKind::Subroutine).unwrap();
        project.set_program_source(index, "; pump
LD M1000
OUT Y1
END
".into());
        let source = project.program_source(index).to_owned();
        let compiler: Compiler = Rc::new({
            let project = project.clone();
            move |text| project.compile_program_text(index, text)
        });
        let edited = with_compiler(compiler.clone(), || {
            edit(&source, SourceLanguage::Il, 0, Edit::SeriesNo("X2".parse().unwrap()))
        })
        .unwrap();
        assert!(edited.contains("LD X2") || edited.contains("AND X2") || edited.contains("X2"));
        assert!(edited.starts_with("; pump
"), "the comment above the network stays");
        // Network numbers are those of the subroutine itself, not of the joined program.
        let commented = with_compiler(compiler, || {
            set_network_comment(&source, SourceLanguage::Il, 0, "Pump on")
        })
        .unwrap();
        assert!(commented.starts_with("; Pump on
"));
        assert!(project.compile_program_text(index, &commented).is_ok());
    }
}
