use egui;

/// SN76489-specific panel decorations (beyond the generic rack panel).
/// For now, the generic rack panel handles everything; this adds a chip description header.
pub fn show_chip_header(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("SN76489 PSG")
                .strong()
                .size(16.0),
        );
        ui.label(
            egui::RichText::new("3 Square + 1 Noise")
                .size(12.0)
                .color(egui::Color32::from_gray(140)),
        );
    });
    ui.separator();
}
