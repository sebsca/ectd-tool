// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, RichText, Stroke};

pub const BACKGROUND: Color32 = Color32::from_rgb(244, 247, 250);
pub const SURFACE: Color32 = Color32::WHITE;
pub const INK: Color32 = Color32::from_rgb(30, 46, 64);
pub const MUTED: Color32 = Color32::from_rgb(91, 108, 126);
pub const LINE: Color32 = Color32::from_rgb(219, 228, 235);
pub const ACCENT: Color32 = Color32::from_rgb(0, 108, 121);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(226, 242, 243);
pub const BLUE: Color32 = Color32::from_rgb(51, 94, 169);
pub const BLUE_SOFT: Color32 = Color32::from_rgb(232, 239, 252);
pub const GREEN: Color32 = Color32::from_rgb(30, 119, 83);
pub const GREEN_SOFT: Color32 = Color32::from_rgb(229, 245, 236);
pub const AMBER: Color32 = Color32::from_rgb(145, 94, 14);
pub const AMBER_SOFT: Color32 = Color32::from_rgb(255, 246, 225);
pub const RED: Color32 = Color32::from_rgb(176, 49, 58);
pub const RED_SOFT: Color32 = Color32::from_rgb(253, 234, 236);

pub fn configure(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (
            egui::TextStyle::Heading,
            FontId::new(20.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(11.0, 6.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.indent = 18.0;
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.scroll.dormant_handle_opacity = 0.35;
    style.spacing.scroll.foreground_color = true;
    style.visuals = egui::Visuals::light();
    style.visuals.override_text_color = Some(INK);
    style.visuals.weak_text_color = Some(MUTED);
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = SURFACE;
    style.visuals.window_stroke = Stroke::new(1.0_f32, LINE);
    style.visuals.window_corner_radius = CornerRadius::same(10);
    style.visuals.menu_corner_radius = CornerRadius::same(7);
    style.visuals.extreme_bg_color = SURFACE;
    style.visuals.faint_bg_color = Color32::from_rgb(248, 250, 252);
    style.visuals.text_edit_bg_color = Some(BACKGROUND);
    style.visuals.code_bg_color = BACKGROUND;
    style.visuals.hyperlink_color = ACCENT;
    style.visuals.warn_fg_color = AMBER;
    style.visuals.error_fg_color = RED;
    style.visuals.selection.bg_fill = ACCENT_SOFT;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    for visual in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        visual.corner_radius = CornerRadius::same(6);
        visual.fg_stroke = Stroke::new(1.0_f32, INK);
        visual.bg_stroke = Stroke::new(1.0_f32, LINE);
        visual.expansion = 0.0;
    }
    style.visuals.widgets.inactive.bg_fill = SURFACE;
    style.visuals.widgets.inactive.weak_bg_fill = SURFACE;
    style.visuals.widgets.hovered.bg_fill = ACCENT_SOFT;
    style.visuals.widgets.hovered.weak_bg_fill = ACCENT_SOFT;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.active.bg_fill = ACCENT_SOFT;
    style.visuals.widgets.active.weak_bg_fill = ACCENT_SOFT;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.open.bg_fill = ACCENT_SOFT;
    style.visuals.widgets.open.weak_bg_fill = ACCENT_SOFT;
    ctx.set_style(style);
}

pub fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
        .fill(ACCENT)
        .stroke(Stroke::NONE)
        .min_size(egui::vec2(0.0, 32.0))
}

pub fn choice(ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(12.0).color(if selected {
            ACCENT
        } else {
            MUTED
        }))
        .fill(if selected {
            ACCENT_SOFT
        } else {
            Color32::TRANSPARENT
        })
        .stroke(Stroke::NONE)
        .selected(selected)
        .min_size(egui::vec2(0.0, 28.0)),
    )
}

pub fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(10.5).strong().color(MUTED));
}

pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32, fill: Color32) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(RichText::new(text).size(10.5).strong().color(color)).truncate(),
            )
            .on_hover_text(text);
        });
}

pub fn status(ui: &mut egui::Ui, text: &str) {
    let upper = text.to_uppercase();
    let (color, fill) = if upper.contains("MISSING") || upper.contains("ERROR") {
        (RED, RED_SOFT)
    } else if upper.contains("WARNING") {
        (AMBER, AMBER_SOFT)
    } else if upper == "OK" || upper == "AVAILABLE" || upper == "NEW" {
        (GREEN, GREEN_SOFT)
    } else if upper == "REPLACE" {
        (BLUE, BLUE_SOFT)
    } else if upper == "APPEND" {
        (ACCENT, ACCENT_SOFT)
    } else if upper.contains("ORPHAN") {
        (BLUE, BLUE_SOFT)
    } else {
        (MUTED, BACKGROUND)
    };
    badge(ui, &upper, color, fill);
}

#[derive(Clone, Copy)]
pub enum Icon {
    Folder,
    Document,
    Layers,
    Code,
    Search,
    Reload,
}

pub fn icon(ui: &mut egui::Ui, kind: Icon, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    draw_icon(ui.painter(), rect, kind, color);
}

pub fn icon_button(ui: &mut egui::Ui, kind: Icon, hint: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
    let visual = ui.style().interact(&response);
    ui.painter().rect(
        rect,
        visual.corner_radius,
        visual.weak_bg_fill,
        visual.bg_stroke,
        egui::StrokeKind::Inside,
    );
    draw_icon(ui.painter(), rect.shrink(8.0), kind, MUTED);
    response.on_hover_text(hint)
}

fn draw_icon(painter: &egui::Painter, rect: egui::Rect, kind: Icon, color: Color32) {
    let p = |x: f32, y: f32| rect.min + egui::vec2(x * rect.width(), y * rect.height());
    let stroke = Stroke::new(1.35_f32, color);
    match kind {
        Icon::Folder => {
            painter.add(egui::Shape::closed_line(
                vec![
                    p(0.08, 0.25),
                    p(0.38, 0.25),
                    p(0.49, 0.36),
                    p(0.92, 0.36),
                    p(0.92, 0.83),
                    p(0.08, 0.83),
                ],
                stroke,
            ));
            painter.line_segment([p(0.08, 0.45), p(0.92, 0.45)], stroke);
        }
        Icon::Document | Icon::Code => {
            painter.add(egui::Shape::closed_line(
                vec![
                    p(0.22, 0.08),
                    p(0.62, 0.08),
                    p(0.83, 0.29),
                    p(0.83, 0.92),
                    p(0.22, 0.92),
                ],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![p(0.61, 0.09), p(0.61, 0.31), p(0.82, 0.31)],
                stroke,
            ));
            for y in [0.49, 0.65, 0.79] {
                painter.line_segment([p(0.36, y), p(0.68, y)], stroke);
            }
        }
        Icon::Layers => {
            for y in [0.28, 0.48, 0.68] {
                painter.add(egui::Shape::line(
                    vec![p(0.12, y), p(0.5, y + 0.18), p(0.88, y)],
                    stroke,
                ));
            }
            painter.add(egui::Shape::line(
                vec![p(0.12, 0.28), p(0.5, 0.1), p(0.88, 0.28)],
                stroke,
            ));
        }
        Icon::Search => {
            painter.circle_stroke(p(0.43, 0.43), rect.width() * 0.28, stroke);
            painter.line_segment([p(0.65, 0.65), p(0.91, 0.91)], stroke);
        }
        Icon::Reload => {
            let points: Vec<_> = (0..=18)
                .map(|i| {
                    let angle = 0.6 + i as f32 / 18.0 * 4.9;
                    p(0.5 + angle.cos() * 0.34, 0.5 + angle.sin() * 0.34)
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
            painter.add(egui::Shape::line(
                vec![p(0.6, 0.14), p(0.82, 0.14), p(0.82, 0.36)],
                stroke,
            ));
        }
    }
}
