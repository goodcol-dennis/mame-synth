use egui;
use rtrb::Producer;
use synth_core::chip::{ParamInfo, ParamKind};
use synth_core::messages::AudioMessage;

use crate::widgets::knob::Knob;

/// Render chip parameters as grouped knobs/toggles.
pub fn render_rack_panel(
    ui: &mut egui::Ui,
    param_infos: &[ParamInfo],
    param_values: &mut [f32],
    audio_tx: &mut Producer<AudioMessage>,
) {
    // Group parameters by their group name (preserve insertion order)
    let mut groups: Vec<(&str, Vec<usize>)> = Vec::new();
    for (idx, info) in param_infos.iter().enumerate() {
        if let Some(group) = groups.iter_mut().find(|(name, _)| *name == info.group.as_str()) {
            group.1.push(idx);
        } else {
            groups.push((info.group.as_str(), vec![idx]));
        }
    }

    for (group_name, indices) in &groups {
        egui::Frame::group(ui.style())
            .fill(egui::Color32::from_rgb(50, 50, 55))
            .corner_radius(4.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(*group_name).strong().size(13.0));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for &idx in indices {
                        let info = &param_infos[idx];
                        let changed = render_param_control(ui, info, &mut param_values[idx]);
                        if changed {
                            let _ = audio_tx.push(AudioMessage::SetParam {
                                param_id: info.id,
                                value: param_values[idx],
                            });
                        }
                    }
                });
            });
        ui.add_space(4.0);
    }
}

fn render_param_control(ui: &mut egui::Ui, info: &ParamInfo, value: &mut f32) -> bool {
    let old_value = *value;

    match &info.kind {
        ParamKind::Continuous { min, max, .. } => {
            ui.add(Knob::new(value, *min, *max, &info.name));
        }
        ParamKind::Discrete { min, max, labels, .. } => {
            if let Some(labels) = labels {
                // Combo box for labeled discrete params
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&info.name).size(10.0));
                    let current = (*value as i32).clamp(*min, *max);
                    let current_label = labels
                        .get((current - min) as usize)
                        .map(|s| s.as_str())
                        .unwrap_or("?");
                    egui::ComboBox::from_id_salt(&info.name)
                        .selected_text(current_label)
                        .width(80.0)
                        .show_ui(ui, |ui| {
                            for i in *min..=*max {
                                let label = labels
                                    .get((i - min) as usize)
                                    .map(|s| s.as_str())
                                    .unwrap_or("?");
                                if ui.selectable_label(current == i, label).clicked() {
                                    *value = i as f32;
                                }
                            }
                        });
                });
            } else {
                // Knob for unlabeled discrete params
                ui.add(Knob::new(value, *min as f32, *max as f32, &info.name).discrete());
            }
        }
        ParamKind::Toggle { .. } => {
            ui.vertical(|ui| {
                let mut on = *value >= 0.5;
                if ui.checkbox(&mut on, &info.name).changed() {
                    *value = if on { 1.0 } else { 0.0 };
                }
            });
        }
    }

    (*value - old_value).abs() > f32::EPSILON
}
