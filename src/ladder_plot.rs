//! Framework-independent Ladder geometry: all conductors meet shared ports.
use opendvp::plc::{Action, Device, Expr, Network};

/// Height of one contact row / output row; leaves room for a device comment above the
/// address and the online value below the symbol.
pub const ROW: f32 = 84.;

#[derive(Clone, Debug)]
pub enum Kind {
    Contact(bool),
    Edge(bool),
    Coil,
    Block,
}
#[derive(Clone, Debug)]
pub struct Symbol {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub label: String,
    pub device: Option<Device>,
    pub kind: Kind,
    pub target: crate::gpui_model::Target,
}
/// Power feeding a conductor: the conjunction of the conditions left of it (empty = the
/// left rail itself). `None` marks the neutral return rail, which never carries power.
pub type Feed = Option<Vec<Expr>>;

fn and(feed: &Feed, e: &Expr) -> Feed {
    let mut terms = feed.clone()?;
    terms.push(e.clone());
    Some(terms)
}

#[derive(Clone, Debug, Default)]
pub struct Plot {
    pub width: f32,
    pub height: f32,
    pub lines: Vec<((f32, f32), (f32, f32))>,
    /// Power source of each entry in `lines`.
    pub feeds: Vec<Feed>,
    pub symbols: Vec<Symbol>,
    condition_count: usize,
}
impl Plot {
    fn line(&mut self, a: (f32, f32), b: (f32, f32), feed: &Feed) {
        if a != b {
            self.lines.push((a, b));
            self.feeds.push(feed.clone());
        }
    }
    fn expr(&mut self, e: &Expr, x: f32, y: f32, feed: &Feed) {
        let (w, _) = measure(e);
        match e {
            Expr::And(a, b) => {
                self.expr(a, x, y, feed);
                self.expr(b, x + measure(a).0, y, &and(feed, a));
            }
            Expr::Or(a, b) => {
                let (wa, ha) = measure(a);
                let (wb, hb) = measure(b);
                let total = ha + hb;
                let ya = y - total / 2. + ha / 2.;
                let yb = y + total / 2. - hb / 2.;
                self.line((x, y), (x + 16., y), feed);
                self.line((x + 16., ya), (x + 16., yb), feed);
                self.expr(a, x + 16., ya, feed);
                self.expr(b, x + 16., yb, feed);
                let after_a = and(feed, a);
                let after_b = and(feed, b);
                let after = and(feed, e);
                self.line((x + 16. + wa, ya), (x + w - 16., ya), &after_a);
                self.line((x + 16. + wb, yb), (x + w - 16., yb), &after_b);
                self.line((x + w - 16., ya), (x + w - 16., yb), &after);
                self.line((x + w - 16., y), (x + w, y), &after);
            }
            Expr::Contact(d, inv) | Expr::Edge(d, inv, _) => {
                let center = x + w / 2.;
                self.line((x, y), (center - 8., y), feed);
                self.line((center + 8., y), (x + w, y), &and(feed, e));
                self.symbols.push(Symbol {
                    x: center,
                    y,
                    width: 16.,
                    height: 22.,
                    label: d.to_string(),
                    device: Some(*d),
                    target: crate::gpui_model::Target::Condition(self.condition_count),
                    kind: if matches!(e, Expr::Edge(..)) {
                        Kind::Edge(*inv)
                    } else {
                        Kind::Contact(*inv)
                    },
                });
                self.condition_count += 1;
            }
            _ => {
                let mut label = String::new();
                crate::ladder_source::emit_expr(e, &mut label);
                self.line((x, y), (x + 12., y), feed);
                self.line((x + w - 12., y), (x + w, y), &and(feed, e));
                self.symbols.push(Symbol {
                    x: x + w / 2.,
                    y,
                    width: w - 24.,
                    height: 34.,
                    label: label.trim().replace('\n', " · "),
                    device: None,
                    kind: Kind::Block,
                    target: crate::gpui_model::Target::Condition(self.condition_count),
                });
                self.condition_count += 1;
            }
        }
    }
}
pub fn measure(e: &Expr) -> (f32, f32) {
    match e {
        Expr::Contact(..) | Expr::Edge(..) => (108., ROW),
        Expr::And(a, b) => {
            let a = measure(a);
            let b = measure(b);
            (a.0 + b.0, a.1.max(b.1))
        }
        Expr::Or(a, b) => {
            let a = measure(a);
            let b = measure(b);
            (a.0.max(b.0) + 32., a.1 + b.1)
        }
        _ => (188., 72.),
    }
}
fn gate(a: &Action) -> bool {
    matches!(
        a,
        Action::SpecialTimer(opendvp::special_timers::TimerAction::Timer { contact: true, .. })
    )
}
pub fn network(network: &Network) -> Plot {
    let (cw, ch) = measure(&network.condition);
    let gates = network.actions.iter().filter(|a| gate(a)).count();
    let width = (cw + 300. + gates as f32 * 180.).max(760.);
    let baseline = if gates == 0 {
        ch.max(network.actions.len() as f32 * ROW) / 2. + 12.
    } else {
        ch / 2. + 12.
    };
    let height = if gates == 0 {
        ch.max(network.actions.len() as f32 * ROW) + 24.
    } else {
        ch + network.actions.len().saturating_sub(1) as f32 * ROW + 24.
    };
    let mut plot = Plot {
        width,
        height,
        ..Default::default()
    };
    let left = 24.;
    let right = width - 24.;
    let rail: Feed = Some(Vec::new());
    plot.line((left, 6.), (left, height - 6.), &rail);
    plot.line((right, 6.), (right, height - 6.), &None);
    plot.expr(&network.condition, left, baseline, &rail);
    // Power reaching the outputs; after an inline timer gate it is not modelled.
    let mut power = and(&rail, &network.condition);
    let mut feed = (width - 222. - gates as f32 * 180., baseline);
    plot.line((left + cw, baseline), feed, &power);
    let first = baseline - (network.actions.len().saturating_sub(1) as f32) * ROW / 2.;
    if gates == 0 && network.actions.len() > 1 {
        plot.line(
            (feed.0, first),
            (feed.0, first + (network.actions.len() - 1) as f32 * ROW),
            &power,
        );
    }
    for (i, action) in network.actions.iter().enumerate() {
        let y = if gates == 0 {
            first + i as f32 * ROW
        } else {
            baseline + i as f32 * ROW
        };
        let inline = gate(action);
        let x = if inline { feed.0 + 92. } else { right - 78. };
        let coil = matches!(action, Action::Out(_));
        let sw = if coil { 30. } else { 136. };
        if gates > 0 {
            plot.line(feed, (feed.0, y), &power);
        }
        plot.line((feed.0, y), (x - sw / 2., y), &power);
        let end = if inline { x + 92. } else { right };
        if inline {
            power = and(&power, &Expr::Saved(usize::MAX));
        }
        // The conductor after an output carries the same power as the output itself,
        // up to the right rail (the rail stays neutral).
        plot.line((x + sw / 2., y), (end, y), &power);
        plot.symbols.push(Symbol {
            x,
            y,
            width: sw,
            height: if coil { 28. } else { 34. },
            label: if coil {
                action.destination().to_string()
            } else {
                action.label()
            },
            device: Some(action.destination()),
            kind: if coil { Kind::Coil } else { Kind::Block },
            target: crate::gpui_model::Target::Action(i),
        });
        if inline {
            feed = (end, y);
        }
    }
    plot
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parallel_branches_share_exact_entry_and_exit_ports() {
        let p = opendvp::plc::compile("LD X0\nOR Y0\nANI X1\nOUT Y0\nEND\n").unwrap();
        let n = network(&p.networks[0]);
        assert_eq!(n.symbols.len(), 4);
        assert!(n.lines.iter().all(|((x, y), (xx, yy))| x == xx || y == yy));
        let x1 = n.symbols.iter().find(|s| s.label == "X1").unwrap();
        assert!(matches!(x1.kind, Kind::Contact(true)));
        assert!(
            n.lines
                .iter()
                .any(|(a, b)| *a == (x1.x + 8., x1.y) && b.0 > x1.x)
        );
        assert!(
            n.symbols
                .iter()
                .all(|s| s.y - s.height / 2. >= 0. && s.y + s.height / 2. <= n.height)
        );
    }
    #[test]
    fn every_conductor_records_the_condition_that_feeds_it() {
        let p = opendvp::plc::compile("LD X0\nOR Y0\nANI X1\nOUT Y0\nEND\n").unwrap();
        let n = network(&p.networks[0]);
        assert_eq!(n.lines.len(), n.feeds.len());
        // Left rail is always fed, the right (return) rail never.
        assert_eq!(n.feeds[0], Some(Vec::new()));
        assert_eq!(n.feeds[1], None);
        // The conductor right of X1 carries (X0 or Y0) and not X1.
        let x1 = n.symbols.iter().find(|s| s.label == "X1").unwrap();
        let after = n
            .lines
            .iter()
            .position(|(a, _)| *a == (x1.x + 8., x1.y))
            .unwrap();
        assert_eq!(n.feeds[after].as_ref().unwrap().len(), 2);
    }
    #[test]
    fn serial_actions_keep_order_and_distinct_neutral_return_rail() {
        let p = opendvp::plc::compile("LD X0\nOUT Y0\nMOV K5 D0\nEND\n").unwrap();
        let n = network(&p.networks[0]);
        assert_eq!(n.symbols[1].label, "Y0");
        assert_eq!(n.symbols[2].label, "MOV K5 D0");
        assert!(n.symbols[1].y < n.symbols[2].y);
    }
}
