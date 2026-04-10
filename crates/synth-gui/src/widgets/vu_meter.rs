use egui::{self, Widget};

use crate::theme;

pub struct VuMeter<'a> {
    level: f32,
    label: &'a str,
}

impl<'a> VuMeter<'a> {
    pub fn new(level: f32, label: &'a str) -> Self {
        VuMeter { level, label }
    }
}

impl<'a> Widget for VuMeter<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let desired_size = egui::vec2(ui.available_width().min(200.0), 20.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();

            // Background
            painter.rect_filled(rect, 2.0, egui::Color32::from_gray(25));

            // Level bar
            let level = self.level.clamp(0.0, 1.0);
            let bar_width = rect.width() * level;
            let bar_rect =
                egui::Rect::from_min_size(rect.min, egui::vec2(bar_width, rect.height()));

            // Color gradient: green -> yellow -> red
            let color = if level < 0.6 {
                theme::VU_GREEN
            } else if level < 0.85 {
                theme::VU_YELLOW
            } else {
                theme::VU_RED
            };
            painter.rect_filled(bar_rect, 2.0, color);

            // Label
            painter.text(
                rect.left_center() + egui::vec2(4.0, 0.0),
                egui::Align2::LEFT_CENTER,
                self.label,
                egui::FontId::proportional(11.0),
                theme::TEXT_PRIMARY,
            );
        }

        response
    }
}
