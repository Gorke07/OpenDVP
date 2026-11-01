//! Printable project documentation: one self-contained HTML file (open in a browser and
//! print, or save as PDF) with project data, hardware, device comments, every network as
//! a ladder drawing plus its IL, and a cross-reference.
use crate::ladder_plot::{self, Kind};
use opendvp::plc::Program;
use opendvp::project::Project;
use std::fmt::Write;

fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// SVG of one network, from the same geometry the ladder editor draws.
fn network_svg(project: &Project, network: &opendvp::plc::Network) -> String {
    let plot = ladder_plot::network(network);
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Consolas, monospace" font-size="12">"#,
        w = plot.width,
        h = plot.height
    );
    for ((x, y), (xx, yy)) in &plot.lines {
        let _ = write!(
            svg,
            r##"<line x1="{x}" y1="{y}" x2="{xx}" y2="{yy}" stroke="#4b5866" stroke-width="1.4"/>"##
        );
    }
    for s in &plot.symbols {
        let (x, y) = (s.x, s.y);
        match s.kind {
            Kind::Contact(inverted) | Kind::Edge(inverted) => {
                let _ = write!(
                    svg,
                    r##"<path d="M{a} {t} V{b} M{c} {t} V{b}" stroke="#1d2733" stroke-width="1.8"/>"##,
                    a = x - 8.,
                    c = x + 8.,
                    t = y - 11.,
                    b = y + 11.
                );
                if matches!(s.kind, Kind::Contact(true)) {
                    let _ = write!(
                        svg,
                        r##"<path d="M{} {} L{} {}" stroke="#1d2733" stroke-width="1.8"/>"##,
                        x - 7.,
                        y + 11.,
                        x + 7.,
                        y - 11.
                    );
                }
                if matches!(s.kind, Kind::Edge(_)) {
                    let mark = if inverted { "↓" } else { "↑" };
                    let _ = write!(
                        svg,
                        r#"<text x="{x}" y="{}" text-anchor="middle">{mark}</text>"#,
                        y + 4.
                    );
                }
            }
            Kind::Coil => {
                let _ = write!(
                    svg,
                    r##"<ellipse cx="{x}" cy="{y}" rx="15" ry="14" fill="white" stroke="#1d2733" stroke-width="1.8"/>"##
                );
            }
            Kind::Block => {
                let _ = write!(
                    svg,
                    r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#f8fafc" stroke="#9aa9bb"/><text x="{x}" y="{}" text-anchor="middle">{}</text>"##,
                    x - s.width / 2.,
                    y - s.height / 2.,
                    s.width,
                    s.height,
                    y + 4.,
                    esc(&s.label)
                );
            }
        }
        if !matches!(s.kind, Kind::Block) {
            let _ = write!(
                svg,
                r#"<text x="{x}" y="{}" text-anchor="middle" font-weight="bold">{}</text>"#,
                y - 18.,
                esc(&s.label)
            );
        }
        if let Some(comment) = s.device.and_then(|d| project.comment(d)) {
            let top = if matches!(s.kind, Kind::Block) {
                y - s.height / 2. - 6.
            } else {
                y - 34.
            };
            let _ = write!(
                svg,
                r##"<text x="{x}" y="{top}" text-anchor="middle" font-family="Segoe UI, sans-serif" font-size="10" fill="#00838f">{}</text>"##,
                esc(comment)
            );
        }
    }
    svg.push_str("</svg>");
    svg
}

pub fn html(project: &Project, program: Option<&Program>) -> String {
    let model = opendvp::devices::find(&project.device);
    let mut out = String::new();
    let _ = write!(
        out,
        r#"<!DOCTYPE html><html lang="tr"><head><meta charset="utf-8"><title>{name} · OpenDVP proje belgesi</title>
<style>
body{{font-family:"Segoe UI",sans-serif;color:#1d2733;margin:32px;}}
h1{{margin:0 0 4px}} h2{{border-bottom:2px solid #00838f;padding-bottom:4px;margin-top:32px}}
table{{border-collapse:collapse;width:100%;margin:8px 0}} th,td{{border:1px solid #d6dce4;padding:4px 8px;text-align:left;font-size:13px}}
th{{background:#f3f5f8}} .muted{{color:#5f6b7a}} pre{{background:#f6f8fa;padding:8px;font-size:12px}}
.net{{border:1px solid #d6dce4;border-radius:6px;margin:12px 0;page-break-inside:avoid}}
.net h3{{margin:0;padding:6px 10px;background:#f3f5f8;font-size:14px}} .net .body{{padding:8px 10px;overflow-x:auto}}
@media print{{body{{margin:12mm}} h2{{page-break-after:avoid}}}}
</style></head><body>
<h1>{name}</h1><div class="muted">OpenDVP proje belgesi · {title} · {lang}</div>"#,
        name = esc(&project.name),
        title = model.map_or(project.device.as_str(), |m| m.title),
        lang = project.language.label()
    );

    out.push_str("<h2>Donanım yapılandırması</h2><table><tr><th>Slot</th><th>Modül</th><th>Açıklama</th><th>Adres</th></tr>");
    let _ = write!(
        out,
        "<tr><td>1</td><td>{}</td><td>CPU · {}</td><td>{} adım</td></tr>",
        model.map_or(project.device.as_str(), |m| m.title),
        model.map_or("", |m| m.io),
        model.map_or(0, |m| m.steps)
    );
    let assignments = opendvp::hardware::assign(&project.hardware).unwrap_or_default();
    for (i, (name, a)) in project.hardware.iter().zip(&assignments).enumerate() {
        let module = opendvp::hardware::find(name);
        let address = match a {
            opendvp::hardware::Assignment::Digital { inputs, outputs } => [
                inputs.map(|r| opendvp::hardware::range_text('X', r)),
                outputs.map(|r| opendvp::hardware::range_text('Y', r)),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", "),
            opendvp::hardware::Assignment::Special { number } => format!("K{number}"),
        };
        let _ = write!(
            out,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            i + 2,
            esc(name),
            esc(module.map_or("", |m| m.summary)),
            esc(&address)
        );
    }
    out.push_str("</table>");

    out.push_str("<h2>Cihaz açıklamaları</h2>");
    if project.comments.is_empty() {
        out.push_str("<p class=\"muted\">Açıklama yok.</p>");
    } else {
        out.push_str("<table><tr><th>Adres</th><th>Açıklama</th></tr>");
        // Device order (X, Y, M, S, T, C, D) rather than text order.
        let mut rows: Vec<(opendvp::plc::Device, &String)> = project
            .comments
            .iter()
            .filter_map(|(d, t)| Some((d.parse().ok()?, t)))
            .collect();
        rows.sort_by_key(|(d, _)| *d);
        for (device, text) in rows {
            let _ = write!(out, "<tr><td>{device}</td><td>{}</td></tr>", esc(text));
        }
        out.push_str("</table>");
    }

    out.push_str("<h2>Program</h2>");
    match program {
        Some(program) => {
            let joined = project.layout().il;
            let source: Vec<&str> = joined.lines().collect();
            for (index, network) in program.networks.iter().enumerate() {
                let mut lines = network.comment.lines();
                let title = lines.next().unwrap_or("");
                let _ = write!(
                    out,
                    r#"<div class="net"><h3>Ağ {} {}</h3><div class="body">"#,
                    index + 1,
                    esc(title)
                );
                for line in lines {
                    let _ = write!(out, "<div class=\"muted\">{}</div>", esc(line));
                }
                out.push_str(&network_svg(project, network));
                let il = source
                    .get(network.line.saturating_sub(1)..network.end_line.min(source.len()))
                    .unwrap_or_default()
                    .join("\n");
                let _ = write!(out, "<pre>{}</pre></div></div>", esc(&il));
            }
        }
        None => {
            let _ = write!(
                out,
                "<p class=\"muted\">Program derlenemedi; kaynak metni:</p><pre>{}</pre>",
                esc(&project.source)
            );
        }
    }

    if let Some(program) = program {
        out.push_str("<h2>Çapraz referans</h2><table><tr><th>Adres</th><th>Açıklama</th><th>Yazıldığı ağlar</th><th>Okunduğu ağlar</th></tr>");
        for (device, uses) in crate::xref::uses(program) {
            let list = |write: bool| {
                let mut nets: Vec<usize> = uses
                    .iter()
                    .filter(|u| u.write == write)
                    .map(|u| u.network + 1)
                    .collect();
                nets.dedup();
                nets.iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let _ = write!(
                out,
                "<tr><td>{device}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(project.comment(device).unwrap_or("")),
                list(true),
                list(false)
            );
        }
        out.push_str("</table>");
    }
    out.push_str("</body></html>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_contains_every_network_hardware_and_comments() {
        let mut project = Project {
            hardware: vec!["DVP16SP11R".into()],
            ..Project::default()
        };
        project.set_comment("X0".parse().unwrap(), "Başlat <buton>");
        let program = project.compile().unwrap();
        let html = html(&project, Some(&program));
        assert_eq!(
            html.matches("<div class=\"net\">").count(),
            program.networks.len()
        );
        assert_eq!(html.matches("<svg").count(), program.networks.len());
        assert!(html.contains("X20–X27"));
        assert!(html.contains("Başlat &lt;buton&gt;"));
        assert!(html.contains("Çapraz referans"));
    }
}

#[cfg(test)]
mod sample {
    #[test]
    #[ignore = "writes a sample document to OPENDVP_REPORT_OUT"]
    fn write_sample_report() {
        let input = std::env::var("OPENDVP_REPORT_IN").unwrap();
        let output = std::env::var("OPENDVP_REPORT_OUT").unwrap();
        let project = opendvp::project::Project::load(std::path::Path::new(&input)).unwrap();
        let program = project.compile().ok();
        std::fs::write(output, super::html(&project, program.as_ref())).unwrap();
    }
}
