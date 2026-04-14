use eframe::egui;
use synth_core::midi_file::MidiSequence;
use synth_core::sid_extract;
use synth_core::vgm::VgmFile;
use synth_core::vgm_extract;

use crate::app::MameSynthApp;
use crate::theme;
use crate::widgets::vu_meter::VuMeter;

pub fn show_transport(app: &mut MameSynthApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if ui.button("Import VGM/SID").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Sound files", &["vgm", "vgz", "sid"])
                .pick_file()
            {
                let is_sid = path.extension().map(|e| e == "sid").unwrap_or(false);

                let extractions: Vec<vgm_extract::VgmExtraction> = if is_sid {
                    match sid_extract::extract_sid_file(&path) {
                        Ok((ext, info)) => {
                            log::info!(
                                "SID: '{}' by {} → {} events",
                                info.name,
                                info.author,
                                ext.events.len()
                            );
                            vec![ext]
                        }
                        Err(e) => {
                            log::error!("Failed to load SID: {}", e);
                            vec![]
                        }
                    }
                } else {
                    match VgmFile::load(&path) {
                        Ok(vgm_file) => {
                            log::info!("Loaded VGM: {}", vgm_file.summary());
                            vgm_extract::extract(&vgm_file)
                        }
                        Err(e) => {
                            log::error!("Failed to load VGM: {}", e);
                            vec![]
                        }
                    }
                };

                let mut total_patches = 0;
                let mut total_events = 0;
                for ext in &extractions {
                    for patch in &ext.patches {
                        if let Err(e) = app.patch_bank.save_patch(patch) {
                            log::error!("Failed to save patch: {}", e);
                        }
                        total_patches += 1;
                    }
                    if !ext.events.is_empty() {
                        let seq = MidiSequence {
                            events: ext.events.clone(),
                            duration_us: ext.duration_us,
                            name: format!(
                                "{} ({})",
                                path.file_stem()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_default(),
                                ext.chip_name
                            ),
                        };
                        total_events += seq.events.len();
                        app.midi_player.load(seq);
                    }
                }
                if total_patches > 0 || total_events > 0 {
                    log::info!(
                        "Import: {} patches, {} note events",
                        total_patches,
                        total_events
                    );
                }
            }
        }

        if ui.button("Open MIDI").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("MIDI files", &["mid", "midi"])
                .pick_file()
            {
                match MidiSequence::load(&path) {
                    Ok(seq) => {
                        log::info!("Loaded MIDI: {} ({} events)", seq.name, seq.events.len());
                        app.midi_player.load(seq);
                    }
                    Err(e) => log::error!("Failed to load MIDI: {}", e),
                }
            }
        }

        if app.midi_player.has_sequence() {
            ui.separator();
            ui.label(
                egui::RichText::new(app.midi_player.sequence_name())
                    .size(11.0)
                    .color(theme::ACCENT),
            );

            if app.midi_player.is_playing() {
                if ui.button("Pause").clicked() {
                    app.midi_player.pause();
                    app.all_notes_off();
                }
            } else {
                let label = if app.midi_player.position_us() > 0 {
                    "Resume"
                } else {
                    "Play"
                };
                if ui.button(label).clicked() {
                    app.midi_player.play();
                }
            }
            if ui.button("Stop").clicked() {
                app.midi_player.stop();
                app.all_notes_off();
            }

            // Progress bar
            let progress = app.midi_player.progress();
            let pos_sec = app.midi_player.position_us() as f32 / 1_000_000.0;
            let dur_sec = app.midi_player.duration_us() as f32 / 1_000_000.0;
            ui.add(
                egui::ProgressBar::new(progress)
                    .text(format!("{:.1}s / {:.1}s", pos_sec, dur_sec))
                    .desired_width(150.0),
            );
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(VuMeter::new(app.peak_right, "R"));
            ui.add(VuMeter::new(app.peak_left, "L"));
            ui.add(crate::widgets::waveform::Waveform::new(&app.waveform_data));
        });
    });
}
