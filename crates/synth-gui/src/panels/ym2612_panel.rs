use egui;

/// YM2612-specific panel decorations.
pub fn show_chip_header(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("YM2612 FM Synthesizer")
                .strong()
                .size(16.0),
        );
        ui.label(
            egui::RichText::new("6-Channel, 4-Operator FM")
                .size(12.0)
                .color(egui::Color32::from_gray(140)),
        );
    });
    ui.separator();
}

/// Draw the FM algorithm topology diagram.
pub fn show_algorithm_diagram(ui: &mut egui::Ui, algorithm: u8) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(200.0, 80.0), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);
    let box_size = egui::vec2(30.0, 20.0);
    let op_color = egui::Color32::from_rgb(0, 160, 220);
    let carrier_color = egui::Color32::from_rgb(220, 160, 0);
    let line_color = egui::Color32::from_gray(120);

    // Simplified: show operator arrangement for each algorithm
    // Algorithms 0-7 define how ops 1-4 connect
    let label = format!("ALG {}", algorithm);
    painter.text(
        rect.center_top() + egui::vec2(0.0, 5.0),
        egui::Align2::CENTER_TOP,
        label,
        egui::FontId::proportional(11.0),
        egui::Color32::from_gray(180),
    );

    // Draw 4 operator boxes
    let y_center = rect.center().y + 8.0;
    let spacing = 42.0;
    let start_x = rect.center().x - spacing * 1.5;

    for i in 0..4u8 {
        let x = start_x + i as f32 * spacing;
        let center = egui::pos2(x, y_center);
        let op_rect = egui::Rect::from_center_size(center, box_size);

        // Carriers are highlighted differently
        let is_carrier = match algorithm {
            0..=3 => i == 3,
            4 => i == 1 || i == 3,
            5 | 6 => i >= 1,
            7 => true,
            _ => false,
        };

        let color = if is_carrier { carrier_color } else { op_color };
        painter.rect_filled(op_rect, 3.0, color);
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            format!("{}", i + 1),
            egui::FontId::proportional(11.0),
            egui::Color32::BLACK,
        );

        // Draw connection line to next op
        if i < 3 {
            let from = egui::pos2(x + box_size.x / 2.0, y_center);
            let to = egui::pos2(x + spacing - box_size.x / 2.0, y_center);
            // Only draw line if ops are connected in this algorithm
            let connected = match algorithm {
                0 => true,             // 1->2->3->4 (serial)
                1 => i != 0,           // (1+2)->3->4
                2 => i != 0,           // (1+2+3)->4
                3 => i == 0 || i == 2, // 1->2, 3->4
                4 => i == 0 || i == 2, // 1->2, 3->4 (parallel out)
                5 => i == 0,           // 1->2, 3, 4
                6 => i == 0,           // 1->2, 3, 4
                7 => false,            // all parallel
                _ => false,
            };
            if connected {
                painter.line_segment([from, to], egui::Stroke::new(1.5, line_color));
            }
        }
    }
}
