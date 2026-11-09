//! Compile messages: the error, warnings found in a program that compiled, and notes.
//! A message with a location jumps to its program and source line when clicked.
use super::*;
use gpui_component::{Icon, IconName};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Severity {
    Error,
    Warning,
    Info,
}
#[derive(Clone)]
pub(super) struct Message {
    pub severity: Severity,
    pub text: String,
    /// Program index (0 = Main) and 1-based line in that program's own source.
    pub target: Option<(usize, usize)>,
}

impl Workbench {
    /// Rebuild the message list after a compile.
    pub(super) fn collect_messages(&mut self) {
        let mut messages = Vec::new();
        if let Some(error) = &self.compile_error {
            messages.push(Message {
                severity: Severity::Error,
                text: error.clone(),
                target: self
                    .compile_mark
                    .as_ref()
                    .map(|(line, _)| (self.active_program, *line)),
            });
        }
        if let Some(program) = &self.program {
            // Several networks writing one device: only the last result survives the scan.
            let uses = crate::xref::uses(program);
            for device in crate::xref::double_coils(program) {
                let target = uses
                    .get(&device)
                    .and_then(|list| list.iter().find(|u| u.write))
                    .and_then(|u| self.project.locate(u.line));
                messages.push(Message {
                    severity: Severity::Warning,
                    text: tfmt!(
                        "{device} birden fazla ağda OUT ile yazılıyor (çoklu bobin).",
                        "{device} is written with OUT in more than one network (multiple coil)."
                    ),
                    target,
                });
            }
            for (device, kind) in crate::xref::unused(program) {
                let target = uses
                    .get(&device)
                    .and_then(|list| list.first())
                    .and_then(|u| self.project.locate(u.line));
                let name = self.device_title(device);
                messages.push(Message {
                    severity: match (kind, device.area) {
                        (crate::xref::Unused::NeverWritten, opendvp::plc::Area::D) => {
                            Severity::Info
                        }
                        (crate::xref::Unused::NeverWritten, _) => Severity::Warning,
                        (crate::xref::Unused::NeverRead, _) => Severity::Info,
                    },
                    text: match kind {
                        crate::xref::Unused::NeverWritten => tfmt!(
                            "{name} okunuyor ama hiçbir yerde yazılmıyor.",
                            "{name} is read but never written anywhere."
                        ),
                        crate::xref::Unused::NeverRead => tfmt!(
                            "{name} yazılıyor ama hiçbir yerde okunmuyor.",
                            "{name} is written but never read."
                        ),
                    },
                    target,
                });
            }
            if let (Some(steps), Some(model)) =
                (self.program_steps, opendvp::devices::find(&self.project.device))
                && steps as u32 > model.steps
            {
                messages.push(Message {
                    severity: Severity::Warning,
                    text: tfmt!(
                        "Program {steps} adım; {} kapasitesi {} adım. PLC'ye yüklenemez.",
                        "The program is {steps} steps; the capacity of {} is {} steps. It cannot be downloaded to the PLC.",
                        model.title,
                        model.steps
                    ),
                    target: None,
                });
            }
            let column = opendvp::devices::find(&self.project.device)
                .map_or("ES2", |m| m.special_column);
            if let Ok(il) = self.project.instruction_list() {
                let devices = opendvp::special_registers::unsupported_in_il(column, &il);
                if !devices.is_empty() {
                    let names: Vec<String> = devices.iter().map(ToString::to_string).collect();
                    messages.push(Message {
                        severity: Severity::Warning,
                        text: tfmt!(
                            "{column} bu özel cihazları desteklemiyor (Delta tablosu): {}",
                            "{column} does not support these special devices (Delta table): {}",
                            names.join(", ")
                        ),
                        target: None,
                    });
                }
            }
            // A symbol nothing refers to, by name or by address.
            let text: String = (0..self.project.program_count())
                .map(|i| self.project.program_source(i).to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join("\n");
            for (name, symbol) in &self.project.symbols {
                let by_name = text
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .any(|word| word == name.to_ascii_lowercase());
                let by_address = symbol
                    .device
                    .as_deref()
                    .and_then(|d| d.parse::<Device>().ok())
                    .is_some_and(|d| program.devices.contains(&d));
                if !by_name && !by_address && self.project.language == SourceLanguage::Il {
                    messages.push(Message {
                        severity: Severity::Info,
                        text: tfmt!("Sembol kullanılmıyor: {name}", "Symbol is not used: {name}"),
                        target: None,
                    });
                }
            }
        }
        for (i, unit) in self.project.units.iter().enumerate() {
            if !unit.enabled {
                messages.push(Message {
                    severity: Severity::Info,
                    text: tfmt!(
                        "{} devre dışı; derlemeye girmiyor.",
                        "{} is disabled; it is not compiled.",
                        unit.name
                    ),
                    target: Some((i + 1, 1)),
                });
            }
        }
        self.messages = messages;
    }

    /// Open the program and line a message points at.
    pub(super) fn jump_to_message(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((program, line)) = self.messages.get(index).and_then(|m| m.target) else {
            return;
        };
        self.select_program(program, cx);
        self.tab = Tab::Source;
        self.jump_line = Some(line);
        cx.notify();
    }

    /// Number of errors and warnings, for the tab label.
    pub(super) fn problem_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.severity != Severity::Info)
            .count()
    }

    pub(super) fn messages_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .id("bottom-messages")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .text_xs();
        if self.messages.is_empty() {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .text_color(rgb(GREEN))
                    .child(Icon::new(IconName::CircleCheck).size(px(14.)))
                    .child(tr("Mesaj yok.")),
            );
        }
        for (index, message) in self.messages.iter().enumerate() {
            let (icon, color, background) = match message.severity {
                Severity::Error => (IconName::CircleX, ERROR, 0xfdecea),
                Severity::Warning => (IconName::TriangleAlert, 0x946113, 0xfff6e0),
                Severity::Info => (IconName::Info, MUTED, 0xf3f5f8),
            };
            let place = message.target.map(|(program, line)| {
                format!("{} · {line}", self.project.program_name(program))
            });
            list = list.child(
                div()
                    .id(SharedString::from(format!("message-{index}")))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py(px(5.))
                    .bg(rgb(background))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .text_color(rgb(color))
                    .when(message.target.is_some(), |row| {
                        row.cursor_pointer()
                            .hover(|style| style.opacity(0.85))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.jump_to_message(index, cx)
                            }))
                    })
                    .child(Icon::new(icon).size(px(14.)))
                    .child(div().flex_1().min_w_0().child(message.text.clone()))
                    .when_some(place, |row, place| {
                        row.child(div().text_color(rgb(MUTED)).child(place))
                    }),
            );
        }
        list.into_any_element()
    }
}
