//! Look and feel matching IconSet Wizard 2: palette, egui visuals, and the
//! icon-button / section helpers used across the window.

use eframe::egui::{self, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, Vec2};

pub const BG: Color32 = Color32::from_rgb(16, 20, 29);
pub const PANEL: Color32 = Color32::from_rgb(20, 26, 37);
pub const LINE: Color32 = Color32::from_rgb(39, 49, 66);
pub const TEXT: Color32 = Color32::from_rgb(231, 237, 246);
pub const MUTED: Color32 = Color32::from_rgb(140, 155, 176);
pub const ACCENT: Color32 = Color32::from_rgb(113, 199, 223);
pub const FILL: Color32 = Color32::from_rgb(29, 38, 53);
pub const HOVER: Color32 = Color32::from_rgb(48, 69, 89);
pub const PRESSED: Color32 = Color32::from_rgb(32, 55, 71);
pub const CARD: Color32 = Color32::from_rgb(26, 35, 49);
pub const CARD_LINE: Color32 = Color32::from_rgb(49, 65, 88);
pub const STATUS_BG: Color32 = Color32::from_rgb(23, 33, 47);
pub const GREEN: Color32 = Color32::from_rgb(120, 186, 161);
pub const AMBER: Color32 = Color32::from_rgb(227, 178, 108);
pub const RED: Color32 = Color32::from_rgb(232, 122, 122);
pub const LOGO_FILL: Color32 = Color32::from_rgb(33, 56, 72);
pub const LOGO_LINE: Color32 = Color32::from_rgb(57, 85, 101);
const ON_ACCENT: Color32 = Color32::from_rgb(16, 39, 50);

pub fn configure(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, CARD_LINE);
    v.extreme_bg_color = BG;
    v.faint_bg_color = FILL;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = PRESSED;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.corner_radius = CornerRadius::same(7);
        w.bg_stroke = Stroke::new(1.0, Color32::from_rgb(53, 65, 84));
        w.bg_fill = FILL;
        w.weak_bg_fill = FILL;
    }
    v.widgets.hovered.bg_fill = HOVER;
    v.widgets.hovered.weak_bg_fill = HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.bg_fill = PRESSED;
    v.widgets.active.weak_bg_fill = PRESSED;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 10.0);
        s.spacing.button_padding = Vec2::new(10.0, 8.0);
        s.spacing.interact_size.y = 30.0;
        for (kind, size) in [
            (egui::TextStyle::Body, 13.0),
            (egui::TextStyle::Button, 12.0),
            (egui::TextStyle::Heading, 18.0),
            (egui::TextStyle::Small, 10.0),
        ] {
            s.text_styles.insert(kind, FontId::proportional(size));
        }
    });
}

/// A small uppercase section label.
pub fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(10.0).color(MUTED).strong());
}

/// Muted secondary text.
pub fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).size(11.0).color(MUTED));
}

/// A filled card frame with the shared border and rounding.
pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, CARD_LINE))
        .corner_radius(9)
        .inner_margin(14)
}

/// A flat panel frame with the shared 1px border (for header / status bars).
pub fn panel_frame(color: Color32, margin: i8) -> egui::Frame {
    egui::Frame::new()
        .fill(color)
        .inner_margin(margin)
        .stroke(Stroke::new(1.0, LINE))
}

/// Draw the rounded logo tile with the shield glyph, as in the ISW2 header.
pub fn logo_tile(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().rect(
        rect,
        12.0,
        LOGO_FILL,
        Stroke::new(1.0, LOGO_LINE),
        egui::StrokeKind::Inside,
    );
    glyph(ui, rect.shrink(size * 0.22), Glyph::Shield, ACCENT);
}

/// Fill and border colors for a custom-drawn control given its state.
pub fn control_colors(r: &egui::Response, primary: bool, enabled: bool) -> (Color32, Color32) {
    if !enabled {
        return (if primary { ACCENT } else { FILL }, LINE);
    }
    let fill = match (r.is_pointer_button_down_on(), r.hovered(), primary) {
        (true, _, true) => Color32::from_rgb(78, 162, 188),
        (true, _, false) => PRESSED,
        (false, true, true) => Color32::from_rgb(153, 223, 243),
        (false, true, false) => HOVER,
        (false, false, true) => ACCENT,
        (false, false, false) => FILL,
    };
    let border = if r.hovered() || r.has_focus() || primary {
        ACCENT
    } else {
        LINE
    };
    (fill, border)
}

#[derive(Clone, Copy)]
pub enum Glyph {
    Shield,
    Folder,
    Lock,
    Check,
    Info,
    Reload,
    Close,
}

/// Draw a stroke glyph defined on a 24x24 grid, scaled into `rect`.
pub fn glyph(ui: &egui::Ui, rect: Rect, kind: Glyph, color: Color32) {
    let p =
        |x: f32, y: f32| rect.min + Vec2::new(x / 24.0 * rect.width(), y / 24.0 * rect.height());
    let s = Stroke::new(1.6, color);
    let line = |pts: &[(f32, f32)]| {
        ui.painter()
            .add(egui::Shape::line(pts.iter().map(|&(x, y)| p(x, y)).collect(), s));
    };
    match kind {
        Glyph::Shield => {
            line(&[
                (12., 2.5),
                (20., 5.5),
                (20., 11.),
                (16.5, 17.5),
                (12., 21.5),
                (7.5, 17.5),
                (4., 11.),
                (4., 5.5),
                (12., 2.5),
            ]);
            line(&[(8.5, 12.), (11., 14.5), (15.5, 9.)]);
        }
        Glyph::Folder => line(&[
            (3., 7.),
            (9., 7.),
            (11., 9.),
            (21., 9.),
            (18.5, 20.),
            (3., 20.),
            (3., 4.),
            (9., 4.),
            (11., 7.),
        ]),
        Glyph::Lock => {
            line(&[(5., 11.), (19., 11.), (19., 21.), (5., 21.), (5., 11.)]);
            line(&[(8., 11.), (8., 7.5), (10., 4.5), (14., 4.5), (16., 7.5), (16., 11.)]);
            line(&[(12., 15.), (12., 17.5)]);
        }
        Glyph::Check => line(&[(4.5, 12.5), (9.5, 17.5), (19.5, 6.5)]),
        Glyph::Info => {
            ui.painter()
                .circle_stroke(p(12., 12.), rect.width() * 9.0 / 24.0, s);
            line(&[(12., 11.), (12., 17.)]);
            line(&[(12., 7.5), (12., 8.2)]);
        }
        Glyph::Reload => {
            line(&[(20., 8.), (20., 13.), (15., 13.)]);
            let pts: Vec<(f32, f32)> = (0..=24)
                .map(|i| {
                    let a = std::f32::consts::PI * 0.15
                        + i as f32 / 24.0 * std::f32::consts::PI * 1.5;
                    (12.0 + 8.0 * a.cos(), 12.0 - 8.0 * a.sin())
                })
                .collect();
            line(&pts);
        }
        Glyph::Close => {
            line(&[(6.5, 6.5), (17.5, 17.5)]);
            line(&[(17.5, 6.5), (6.5, 17.5)]);
        }
    }
}

/// An icon button in the IconSet Wizard 2 style: 36 px tall, rounded 7 px,
/// accent fill when `primary`. Returns the click response.
pub fn icon_button(
    ui: &mut egui::Ui,
    kind: Glyph,
    text: &str,
    enabled: bool,
    primary: bool,
) -> egui::Response {
    let color = if primary { ON_ACCENT } else { TEXT };
    let galley = ui
        .painter()
        .layout_no_wrap(text.into(), FontId::proportional(12.0), color);
    let width = if text.is_empty() {
        36.0
    } else {
        galley.size().x + 49.0
    };
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 36.0), sense);
    let alpha = if enabled { 1.0 } else { 0.35 };
    let (fill, border) = control_colors(&response, primary, enabled);
    ui.painter().rect(
        rect,
        7.0,
        fill.gamma_multiply(alpha),
        Stroke::new(1.0, border.gamma_multiply(alpha)),
        egui::StrokeKind::Inside,
    );
    glyph(
        ui,
        Rect::from_min_size(rect.min + Vec2::new(10., 9.), Vec2::splat(18.)),
        kind,
        color.gamma_multiply(alpha),
    );
    if !text.is_empty() {
        let pos = rect.min + Vec2::new(35., (36. - galley.size().y) / 2.);
        ui.painter()
            .galley_with_override_text_color(pos, galley, color.gamma_multiply(alpha));
    }
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}
