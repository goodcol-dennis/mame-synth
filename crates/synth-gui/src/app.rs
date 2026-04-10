use eframe::egui;
use rtrb::{Consumer, Producer};
use synth_core::chip::{param_info_for_chip, ChipId, VoiceMode};
use synth_core::messages::{AudioMessage, GuiMessage};
use synth_core::midi::MidiHandler;

use crate::panels;
use crate::rack_panel;
use crate::theme;
use crate::widgets::keyboard::PianoKeyboard;
use crate::widgets::vu_meter::VuMeter;

pub struct MameSynthApp {
    pub audio_tx: Producer<AudioMessage>,
    pub gui_rx: Consumer<GuiMessage>,

    active_chip: ChipId,
    param_infos: Vec<synth_core::chip::ParamInfo>,
    param_values: Vec<f32>,

    midi_handler: MidiHandler,
    midi_ports: Vec<String>,
    selected_midi_port: Option<usize>,

    peak_left: f32,
    peak_right: f32,

    held_keys: Vec<u8>,
    keyboard_octave: u8,
    theme_applied: bool,
    voice_mode_index: usize, // 0=Poly, 1=Mono, 2=Unison
    unison_detune: f32,
}

impl MameSynthApp {
    pub fn new(audio_tx: Producer<AudioMessage>, gui_rx: Consumer<GuiMessage>) -> Self {
        let active_chip = ChipId::Sn76489;
        let param_infos = param_info_for_chip(active_chip);
        let param_values: Vec<f32> = param_infos.iter().map(|p| p.kind.default_value()).collect();
        let midi_ports = MidiHandler::scan_ports();

        MameSynthApp {
            audio_tx,
            gui_rx,
            active_chip,
            param_infos,
            param_values,
            midi_handler: MidiHandler::new(),
            midi_ports,
            selected_midi_port: None,
            peak_left: 0.0,
            peak_right: 0.0,
            held_keys: Vec::new(),
            keyboard_octave: 4,
            theme_applied: false,
            voice_mode_index: 0,
            unison_detune: 15.0,
        }
    }

    fn switch_chip(&mut self, new_chip: ChipId) {
        if new_chip == self.active_chip {
            return;
        }
        self.active_chip = new_chip;
        self.param_infos = param_info_for_chip(new_chip);
        self.param_values = self
            .param_infos
            .iter()
            .map(|p| p.kind.default_value())
            .collect();
        let _ = self.audio_tx.push(AudioMessage::SwitchChip(new_chip));

        // Release any held notes
        for note in self.held_keys.drain(..) {
            let _ = self.audio_tx.push(AudioMessage::NoteOff { note });
        }
    }

    fn drain_gui_messages(&mut self) {
        while let Ok(msg) = self.gui_rx.pop() {
            match msg {
                GuiMessage::PeakLevel { left, right } => {
                    // Take max so we don't miss transients between frames
                    self.peak_left = self.peak_left.max(left);
                    self.peak_right = self.peak_right.max(right);
                }
            }
        }
    }

    fn handle_computer_keyboard(&mut self, ctx: &egui::Context) {
        // Map computer keys to MIDI notes
        let key_map: &[(egui::Key, u8)] = &[
            (egui::Key::Z, 0),  // C
            (egui::Key::S, 1),  // C#
            (egui::Key::X, 2),  // D
            (egui::Key::D, 3),  // D#
            (egui::Key::C, 4),  // E
            (egui::Key::V, 5),  // F
            (egui::Key::G, 6),  // F#
            (egui::Key::B, 7),  // G
            (egui::Key::H, 8),  // G#
            (egui::Key::N, 9),  // A
            (egui::Key::J, 10), // A#
            (egui::Key::M, 11), // B
        ];

        // Process raw input events for keyboard.
        // Dump all key events to understand what Wayland/egui is sending.
        let octave = self.keyboard_octave;
        let raw_events: Vec<(u8, bool, bool)> = ctx.input(|input| {
            let mut evts = Vec::new();
            for event in &input.events {
                if let egui::Event::Key {
                    key,
                    pressed,
                    repeat,
                    ..
                } = event
                {
                    for (mapped_key, semitone) in key_map {
                        if key == mapped_key {
                            let midi_note = octave * 12 + semitone;
                            evts.push((midi_note, *pressed, *repeat));
                        }
                    }
                }
            }
            evts
        });

        if !raw_events.is_empty() {
            let desc: Vec<String> = raw_events
                .iter()
                .map(|(n, p, r)| {
                    format!(
                        "{}{}{}",
                        if *p { "+" } else { "-" },
                        n,
                        if *r { "R" } else { "" }
                    )
                })
                .collect();
            log::info!("Raw events: {}", desc.join(" "));
        }

        // Only process non-repeat events. For keys with both press and release
        // in the same frame (non-repeat), keep the note ON — the release is spurious.
        for &(midi_note, pressed, repeat) in &raw_events {
            if repeat {
                continue;
            }
            if pressed && !self.held_keys.contains(&midi_note) {
                self.held_keys.push(midi_note);
                match self.audio_tx.push(AudioMessage::NoteOn {
                    note: midi_note,
                    velocity: 100,
                }) {
                    Ok(()) => log::info!("Sent NoteOn {}", midi_note),
                    Err(e) => log::error!("Failed to send NoteOn: {:?}", e),
                }
            } else if !pressed && self.held_keys.contains(&midi_note) {
                // Check if there's also a press for this note in the same frame
                let also_pressed = raw_events
                    .iter()
                    .any(|&(n, p, r)| n == midi_note && p && !r);
                if also_pressed {
                    // Both press and release in same frame — keep it ON
                    log::info!(
                        "Suppressing release for {} (press in same frame)",
                        midi_note
                    );
                    continue;
                }
                self.held_keys.retain(|&n| n != midi_note);
                let _ = self
                    .audio_tx
                    .push(AudioMessage::NoteOff { note: midi_note });
                log::info!("Sent NoteOff {}", midi_note);
            }
        }
    }
}

impl eframe::App for MameSynthApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.theme_applied {
            theme::apply_theme(ctx);
            self.theme_applied = true;
        }

        self.drain_gui_messages();
        self.handle_computer_keyboard(ctx);

        // Decay VU meters smoothly
        let decay = 0.85f32;
        self.peak_left *= decay;
        self.peak_right *= decay;

        // Repaint at ~60fps for VU meters, not max framerate
        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.label(
                    egui::RichText::new("mame-synth")
                        .strong()
                        .size(14.0)
                        .color(theme::ACCENT),
                );
                ui.separator();

                // MIDI port selector
                ui.label("MIDI:");
                let current_name = self
                    .selected_midi_port
                    .and_then(|i| self.midi_ports.get(i))
                    .map(|s| s.as_str())
                    .unwrap_or("(none)");
                egui::ComboBox::from_id_salt("midi_port")
                    .selected_text(current_name)
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.selected_midi_port.is_none(), "(none)")
                            .clicked()
                        {
                            self.midi_handler.disconnect();
                            self.selected_midi_port = None;
                        }
                        for (i, name) in self.midi_ports.iter().enumerate() {
                            if ui
                                .selectable_label(self.selected_midi_port == Some(i), name)
                                .clicked()
                            {
                                // Need a new producer for this MIDI connection
                                // For now, MIDI uses the same audio_tx — but rtrb is SPSC,
                                // so we'll need to address this. For v0.1, skip MIDI connection
                                // from GUI (use only computer keyboard).
                                self.selected_midi_port = Some(i);
                            }
                        }
                    });

                if ui.button("Rescan").clicked() {
                    self.midi_ports = MidiHandler::scan_ports();
                }

                ui.separator();
                ui.label(format!("Octave: {}", self.keyboard_octave));
                if ui.button("-").clicked() && self.keyboard_octave > 1 {
                    self.keyboard_octave -= 1;
                }
                if ui.button("+").clicked() && self.keyboard_octave < 7 {
                    self.keyboard_octave += 1;
                }

                ui.separator();
                let mode_names = ["Poly", "Mono", "Unison"];
                let current_mode = mode_names[self.voice_mode_index];
                egui::ComboBox::from_id_salt("voice_mode")
                    .selected_text(current_mode)
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in mode_names.iter().enumerate() {
                            if ui
                                .selectable_label(self.voice_mode_index == i, *name)
                                .clicked()
                            {
                                self.voice_mode_index = i;
                                let mode = match i {
                                    1 => VoiceMode::Mono,
                                    2 => VoiceMode::Unison {
                                        detune_cents: self.unison_detune,
                                    },
                                    _ => VoiceMode::Poly,
                                };
                                let _ = self.audio_tx.push(AudioMessage::SetVoiceMode(mode));
                            }
                        }
                    });

                if self.voice_mode_index == 2 {
                    ui.label("Detune:");
                    let prev = self.unison_detune;
                    ui.add(
                        egui::DragValue::new(&mut self.unison_detune)
                            .range(0.0..=50.0)
                            .speed(0.5)
                            .suffix(" ct"),
                    );
                    if (self.unison_detune - prev).abs() > 0.1 {
                        let _ = self
                            .audio_tx
                            .push(AudioMessage::SetVoiceMode(VoiceMode::Unison {
                                detune_cents: self.unison_detune,
                            }));
                    }
                }
            });
        });

        // Bottom panel: keyboard
        egui::TopBottomPanel::bottom("keyboard_panel")
            .min_height(130.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);

                // VU meters
                ui.horizontal(|ui| {
                    ui.add(VuMeter::new(self.peak_left, "L"));
                    ui.add(VuMeter::new(self.peak_right, "R"));
                });
                ui.add_space(4.0);

                // Virtual keyboard
                let keyboard = PianoKeyboard::new(self.keyboard_octave, 4);
                let result = keyboard.show(ui, &self.held_keys);

                if let Some(note) = result.note_on {
                    if !self.held_keys.contains(&note) {
                        self.held_keys.push(note);
                        let _ = self.audio_tx.push(AudioMessage::NoteOn {
                            note,
                            velocity: 100,
                        });
                    }
                }
                if let Some(note) = result.note_off {
                    self.held_keys.retain(|&n| n != note);
                    let _ = self.audio_tx.push(AudioMessage::NoteOff { note });
                }
            });

        // Central panel: chip selector + rack
        egui::CentralPanel::default().show(ctx, |ui| {
            // Chip selector
            ui.horizontal(|ui| {
                for chip in ChipId::all() {
                    let selected = *chip == self.active_chip;
                    let text = egui::RichText::new(chip.display_name()).size(14.0).strong();
                    if ui.selectable_label(selected, text).clicked() {
                        self.switch_chip(*chip);
                    }
                }
            });
            ui.separator();

            // Chip-specific header
            match self.active_chip {
                ChipId::Sn76489 => panels::sn76489_panel::show_chip_header(ui),
                ChipId::Ym2612 => {
                    panels::ym2612_panel::show_chip_header(ui);
                    let algo_value = self.param_values.first().copied().unwrap_or(0.0);
                    panels::ym2612_panel::show_algorithm_diagram(ui, algo_value as u8);
                }
                ChipId::Sid6581 => panels::sid6581_panel::show_chip_header(ui),
            }

            // Scrollable rack panel
            egui::ScrollArea::vertical().show(ui, |ui| {
                rack_panel::render_rack_panel(
                    ui,
                    &self.param_infos,
                    &mut self.param_values,
                    &mut self.audio_tx,
                );
            });
        });
    }
}
