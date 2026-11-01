use opendvp::plc::Expr;
pub fn emit_expr(e: &Expr, out: &mut String) {
    match e {
        Expr::IndexedBitNumber(p) => out.push_str(&format!("{}\n", p.label("LD"))),
        Expr::IndexedBit(contact) => out.push_str(&format!("{}\n", contact.label("LD"))),
        Expr::IndexedComparison(predicate) => out.push_str(&format!("{}\n", predicate.label("LD"))),
        Expr::IndexedFloatComparison(predicate) => {
            out.push_str(&format!("{}\n", predicate.label("LD")))
        }
        Expr::Float(predicate) => out.push_str(&format!("{}\n", predicate.label("LD"))),
        Expr::Numeric(predicate) => out.push_str(&format!("{}\n", predicate.label("LD"))),
        Expr::Invert(inner) => {
            emit_expr(inner, out);
            out.push_str("INV\n");
        }
        Expr::Compare(op, a, b) => out.push_str(&format!("LD{} {a} {b}\n", op.symbol())),
        Expr::CompareWide(op, a, b) => out.push_str(&format!("DLD{} {a} {b}\n", op.symbol())),
        Expr::Saved(_) => {}
        Expr::Contact(d, inv) => {
            out.push_str(&format!("{} {d}\n", if *inv { "LDI" } else { "LD" }))
        }
        Expr::Edge(d, rising, _) => {
            out.push_str(&format!("{} {d}\n", if *rising { "LDP" } else { "LDF" }))
        }
        Expr::Trigger(inner, rising, _) => {
            emit_expr(inner, out);
            out.push_str(if *rising { "NP\n" } else { "PN\n" });
        }
        Expr::And(a, b) | Expr::Or(a, b) => {
            emit_expr(a, out);
            let prefix = if matches!(e, Expr::And(..)) {
                "AND"
            } else {
                "OR"
            };
            // Preserve a primitive contact's own read/shortcut contract;
            // lowering ANDZ to LDZ+ANB would unconditionally read its operands.
            match b.as_ref() {
                Expr::IndexedBitNumber(p) => out.push_str(&format!("{}\n", p.label(prefix))),
                Expr::IndexedBit(contact) => out.push_str(&format!("{}\n", contact.label(prefix))),
                Expr::IndexedComparison(p) => out.push_str(&format!("{}\n", p.label(prefix))),
                Expr::IndexedFloatComparison(p) => out.push_str(&format!("{}\n", p.label(prefix))),
                Expr::Numeric(
                    p @ (opendvp::numeric_contacts::NumericPredicate::Difference(..)
                    | opendvp::numeric_contacts::NumericPredicate::DifferenceWide(..)),
                ) => {
                    out.push_str(&format!("{}\n", p.label(prefix)));
                }
                _ => {
                    emit_expr(b, out);
                    out.push_str(if prefix == "AND" { "ANB\n" } else { "ORB\n" });
                }
            }
        }
    }
}
