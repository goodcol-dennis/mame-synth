use egui;

pub fn show_chip_header(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("MOS 6581 SID").strong().size(16.0));
        ui.label(
            egui::RichText::new("3 Voice + ADSR (Commodore 64)")
                .size(12.0)
                .color(egui::Color32::from_gray(140)),
        );
    });
    ui.separator();
}
