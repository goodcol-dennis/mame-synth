use egui::Color32;

// Rack-style dark theme colors
pub const BG_DARK: Color32 = Color32::from_rgb(30, 30, 35);
pub const BG_PANEL: Color32 = Color32::from_rgb(45, 45, 50);
pub const BG_GROUP: Color32 = Color32::from_rgb(55, 55, 60);
pub const KNOB_BG: Color32 = Color32::from_rgb(40, 40, 45);
pub const KNOB_RING: Color32 = Color32::from_rgb(80, 80, 85);
pub const KNOB_INDICATOR: Color32 = Color32::from_rgb(0, 200, 255);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(220, 220, 225);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(150, 150, 155);
pub const ACCENT: Color32 = Color32::from_rgb(0, 200, 255);
pub const VU_GREEN: Color32 = Color32::from_rgb(0, 200, 80);
pub const VU_YELLOW: Color32 = Color32::from_rgb(220, 200, 0);
pub const VU_RED: Color32 = Color32::from_rgb(220, 40, 40);
pub const KEY_WHITE: Color32 = Color32::from_rgb(240, 240, 240);
pub const KEY_BLACK: Color32 = Color32::from_rgb(30, 30, 30);
pub const KEY_PRESSED: Color32 = Color32::from_rgb(0, 160, 220);

pub fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals.dark_mode = true;
    style.visuals.panel_fill = BG_DARK;
    style.visuals.window_fill = BG_DARK;
    style.visuals.override_text_color = Some(TEXT_PRIMARY);
    ctx.set_style(style);
}
