use egui::{self, Response, Sense, Widget};
use std::f32::consts::PI;

use crate::theme;

pub struct Knob<'a> {
    value: &'a mut f32,
    min: f32,
    max: f32,
    label: &'a str,
    diameter: f32,
    is_discrete: bool,
}

impl<'a> Knob<'a> {
    pub fn new(value: &'a mut f32, min: f32, max: f32, label: &'a str) -> Self {
        Knob {
            value,
            min,
            max,
            label,
            diameter: 48.0,
            is_discrete: false,
        }
    }

    pub fn discrete(mut self) -> Self {
        self.is_discrete = true;
        self
    }
}

impl<'a> Widget for Knob<'a> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let desired_size = egui::vec2(self.diameter + 16.0, self.diameter + 28.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::drag());

        if response.dragged() {
            let delta = -response.drag_delta().y * (self.max - self.min) / 200.0;
            *self.value = (*self.value + delta).clamp(self.min, self.max);
            if self.is_discrete {
                *self.value = self.value.round();
            }
        }

        // Scroll wheel support
        if response.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                let delta = scroll.signum() * (self.max - self.min) / 50.0;
                *self.value = (*self.value + delta).clamp(self.min, self.max);
                if self.is_discrete {
                    *self.value = self.value.round();
                }
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let center = rect.center_top() + egui::vec2(0.0, self.diameter / 2.0 + 2.0);
            let radius = self.diameter / 2.0;

            // Background circle
            painter.circle_filled(center, radius, theme::KNOB_BG);
            painter.circle_stroke(
                center,
                radius,
                egui::Stroke::new(2.0, theme::KNOB_RING),
            );

            // Arc from start angle to current value
            let normalized = (*self.value - self.min) / (self.max - self.min);
            let start_angle = PI * 0.75;
            let end_angle = start_angle + normalized * PI * 1.5;

            // Draw arc segments
            let arc_radius = radius - 3.0;
            let segments = 32;
            let arc_extent = normalized * PI * 1.5;
            for i in 0..segments {
                let t0 = i as f32 / segments as f32;
                let t1 = (i + 1) as f32 / segments as f32;
                if t1 * PI * 1.5 > arc_extent {
                    break;
                }
                let a0 = start_angle + t0 * PI * 1.5;
                let a1 = start_angle + t1 * PI * 1.5;
                let p0 = center + egui::vec2(a0.cos(), a0.sin()) * arc_radius;
                let p1 = center + egui::vec2(a1.cos(), a1.sin()) * arc_radius;
                painter.line_segment([p0, p1], egui::Stroke::new(3.0, theme::KNOB_INDICATOR));
            }

            // Indicator line
            let tip = center + egui::vec2(end_angle.cos(), end_angle.sin()) * (radius - 6.0);
            painter.line_segment(
                [center, tip],
                egui::Stroke::new(2.0, theme::KNOB_INDICATOR),
            );

            // Value text inside knob
            let value_text = if self.is_discrete {
                format!("{}", *self.value as i32)
            } else {
                format!("{:.1}", *self.value)
            };
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                value_text,
                egui::FontId::proportional(10.0),
                theme::TEXT_PRIMARY,
            );

            // Label below
            painter.text(
                egui::pos2(rect.center().x, rect.bottom()),
                egui::Align2::CENTER_BOTTOM,
                self.label,
                egui::FontId::proportional(10.0),
                theme::TEXT_SECONDARY,
            );
        }

        response
    }
}
