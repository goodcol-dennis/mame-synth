pub struct Waveform<'a> {
    samples: &'a [f32; 128],
}

impl<'a> Waveform<'a> {
    pub fn new(samples: &'a [f32; 128]) -> Self {
        Waveform { samples }
    }
}

impl<'a> egui::Widget for Waveform<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let desired_size = egui::vec2(200.0, 60.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            // Background
            painter.rect_filled(rect, 2.0, egui::Color32::from_gray(20));

            // Draw waveform as connected line segments
            let center_y = rect.center().y;
            let half_height = rect.height() / 2.0 * 0.9;
            let step = rect.width() / 127.0;

            for i in 0..127 {
                let x0 = rect.left() + i as f32 * step;
                let x1 = rect.left() + (i + 1) as f32 * step;
                let y0 = center_y - self.samples[i] * half_height;
                let y1 = center_y - self.samples[i + 1] * half_height;
                painter.line_segment(
                    [egui::pos2(x0, y0), egui::pos2(x1, y1)],
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 200, 255)),
                );
            }

            // Center line
            painter.line_segment(
                [
                    egui::pos2(rect.left(), center_y),
                    egui::pos2(rect.right(), center_y),
                ],
                egui::Stroke::new(0.5, egui::Color32::from_gray(60)),
            );
        }
        response
    }
}
