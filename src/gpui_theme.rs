// Light/dark palette. Every colour in the UI goes through `rgb`, which remaps the light palette to
// a dark one when dark mode is on, so the whole workbench switches without touching each call site.
use gpui::Rgba;
use std::sync::atomic::{AtomicBool, Ordering};

static DARK: AtomicBool = AtomicBool::new(false);

pub fn set_dark(on: bool) {
    DARK.store(on, Ordering::Relaxed);
}

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Switches the palette and the component library's theme (buttons, inputs, scale) together.
pub fn apply(dark: bool, cx: &mut gpui::App) {
    use gpui_component::{Theme, ThemeMode};
    set_dark(dark);
    Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
    // `Theme::update` keeps the colour tokens (which buttons read) in step with the colours.
    Theme::update(cx, |theme| {
        let accent = if dark { gpui::rgb(0x1fa5b3) } else { gpui::rgb(0x00838f) };
        theme.colors.primary = accent.into();
        theme.colors.primary_hover = if dark { gpui::rgb(0x2bb8c6) } else { gpui::rgb(0x006e78) }.into();
        theme.colors.primary_active = if dark { gpui::rgb(0x178a96) } else { gpui::rgb(0x005a62) }.into();
        theme.colors.primary_foreground = gpui::rgb(0xffffff).into();
        let c = &mut theme.colors;
        c.button_primary = c.primary;
        c.button_primary_hover = c.primary_hover;
        c.button_primary_active = c.primary_active;
        c.button_primary_foreground = c.primary_foreground;
        c.button_success = gpui::rgb(0x22c55e).into();
        c.button_success_hover = gpui::rgb(0x16a34a).into();
        c.button_success_active = gpui::rgb(0x15803d).into();
        c.button_success_foreground = gpui::rgb(0xffffff).into();
        c.button_warning = gpui::rgb(0xf59e0b).into();
        c.button_warning_hover = gpui::rgb(0xd97706).into();
        c.button_warning_active = gpui::rgb(0xb45309).into();
        c.button_warning_foreground = gpui::rgb(0xffffff).into();
        c.button_danger = gpui::rgb(0xdc2626).into();
        c.button_danger_hover = gpui::rgb(0xb91c1c).into();
        c.button_danger_active = gpui::rgb(0x991b1b).into();
        c.button_danger_foreground = gpui::rgb(0xffffff).into();
        theme.font_size = gpui::px(16.0 * f32::from(opendvp::settings::get().ui_scale) / 100.0);
        theme.radius = gpui::px(5.);
        theme.shadow = false;
    });
}

/// Drop-in replacement for `gpui::rgb`.
pub fn rgb(hex: u32) -> Rgba {
    gpui::rgb(if is_dark() { dark(hex) } else { hex })
}

fn dark(hex: u32) -> u32 {
    match hex {
        // palette constants (see gpui_workbench.rs)
        0xeceff3 => 0x11161c, // BG
        0xfffffe => 0x1a2129, // PANEL (pure white stays white: it is text on coloured fills)
        0xd6dce4 => 0x323d4a, // LINE
        0x1d2733 => 0xe6ebf1, // TEXT
        0x5f6b7a => 0x97a4b4, // MUTED
        0x15803d => 0x4ade80, // GREEN
        0x00838f => 0x2bb8c6, // ACCENT
        0xc2410c => 0xfb923c, // AMBER
        0xb42318 => 0xf87171, // ERROR
        0xfff3e2 => 0x3a2a14, // ONLINE_SOFT
        // text on the (always dark) title bar
        0xe8eef4 | 0x9fb0c2 => hex,
        // dark text on light module cards
        0x24364c => 0xd5dfeb,
        0x52647a => 0x9fb0c2,
        0xb45309 => 0xfbbf24,
        0xad3b35 => 0xf08a84,
        other => invert_light(other),
    }
}

/// Light tints become dark tints of the same hue; mid and dark colours (accents, the dark title
/// bar) are left alone.
fn invert_light(hex: u32) -> u32 {
    let r = ((hex >> 16) & 0xff) as f32 / 255.0;
    let g = ((hex >> 8) & 0xff) as f32 / 255.0;
    let b = (hex & 0xff) as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if l < 0.6 || hex == 0xffffff {
        return hex;
    }
    let d = max - min;
    let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let nl = 0.08 + (1.0 - l) * 0.55;
    let ns = s * 0.6;
    let c = (1.0 - (2.0 * nl - 1.0).abs()) * ns;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let m = nl - c / 2.0;
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let ch = |v: f32| (((v + m) * 255.0).round() as u32).min(255);
    (ch(r) << 16) | (ch(g) << 8) | ch(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_tints_get_dark_and_dark_stay() {
        assert_eq!(dark(0x18222d), 0x18222d);
        assert_eq!(dark(0xffffff), 0xffffff);
        let tint = dark(0xedf0f4);
        assert!(tint >> 16 < 0x40 && tint & 0xff < 0x40, "{tint:06x}");
    }
}
