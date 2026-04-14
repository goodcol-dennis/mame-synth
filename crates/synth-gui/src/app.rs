use eframe::egui;
use rtrb::{Consumer, Producer};
use synth_core::chip::{param_info_for_chip, ChipId, VoiceMode};
use synth_core::messages::{AudioMessage, GuiMessage};
use synth_core::midi::MidiHandler;
use synth_core::midi_file::MidiPlayer;
use synth_core::patch::{Patch, PatchBank};

use crate::panels;
use crate::rack_panel;
use crate::theme;
use crate::widgets::keyboard::PianoKeyboard;

pub struct MameSynthApp {
    pub audio_tx: Producer<AudioMessage>,
    pub(crate) gui_rx: Consumer<GuiMessage>,

    pub(crate) active_chip: ChipId,
    pub(crate) param_infos: Vec<synth_core::chip::ParamInfo>,
    pub(crate) param_values: Vec<f32>,

    pub(crate) midi_handler: MidiHandler,
    pub(crate) midi_ports: Vec<String>,
    pub(crate) selected_midi_port: Option<usize>,

    pub(crate) peak_left: f32,
    pub(crate) peak_right: f32,

    pub(crate) held_keys: Vec<u8>,
    pub(crate) keyboard_octave: u8,
    pub(crate) theme_applied: bool,
    pub(crate) voice_mode_index: usize, // 0=Poly, 1=Mono, 2=Unison
    pub(crate) unison_detune: f32,
    pub(crate) chip_count: u8,
    pub(crate) mouse_note: Option<u8>,
    pub(crate) macro_index: u8,
    pub(crate) waveform_data: [f32; 128],
    pub(crate) patch_bank: PatchBank,
    pub(crate) selected_patch: Option<usize>,
    pub(crate) save_patch_name: String,
    pub(crate) show_save_dialog: bool,
    pub(crate) midi_player: MidiPlayer,
}

fn voices_per_chip(chip: synth_core::chip::ChipId) -> u8 {
    use synth_core::chip::ChipId;
    match chip {
        ChipId::Sn76489 => 3,
        ChipId::Ym2612 => 6,
        ChipId::Sid6581 => 3,
        ChipId::Ay8910 => 3,
        ChipId::Ricoh2a03 => 4,
        ChipId::Pokey => 4,
        ChipId::Ym2151 => 8,
        ChipId::Ym3812 => 9,
        ChipId::Ymf262 => 18,
        ChipId::Scc => 5,
        ChipId::NamcoWsg => 3,
    }
}

fn dirs() -> std::path::PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            std::path::PathBuf::from(home).join(".local/share")
        });
    base.join("mame-synth/patches")
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
            chip_count: 1,
            mouse_note: None,
            macro_index: 255,
            waveform_data: [0.0; 128],
            patch_bank: {
                let dir = dirs();
                let mut bank = PatchBank::new(dir);
                bank.ensure_factory_presets();
                bank
            },
            selected_patch: None,
            save_patch_name: String::new(),
            show_save_dialog: false,
            midi_player: MidiPlayer::new(),
        }
    }

    /// Silence everything — reset the active chip.
    pub(crate) fn all_notes_off(&mut self) {
        let _ = self.audio_tx.push(AudioMessage::Reset);
        self.held_keys.clear();
    }

    pub(crate) fn switch_chip(&mut self, new_chip: ChipId) {
        if new_chip == self.active_chip {
            return;
        }
        self.active_chip = new_chip;
        self.chip_count = 1;
        self.macro_index = 255;
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

    fn load_patch(&mut self, patch: &Patch) {
        // Switch chip if needed
        if let Some(chip_id) = patch.chip_id() {
            if chip_id != self.active_chip {
                self.switch_chip(chip_id);
            }
        }

        // Apply voice mode
        let mode = patch.voice_mode();
        let _ = self.audio_tx.push(AudioMessage::SetVoiceMode(mode));
        match mode {
            VoiceMode::Mono => self.voice_mode_index = 1,
            VoiceMode::Poly => self.voice_mode_index = 0,
            VoiceMode::Unison { detune_cents } => {
                self.voice_mode_index = 2;
                self.unison_detune = detune_cents;
            }
        }

        // Apply params
        for (i, info) in self.param_infos.iter().enumerate() {
            if let Some(val) = patch.get_param(info.id) {
                self.param_values[i] = val;
                let _ = self.audio_tx.push(AudioMessage::SetParam {
                    param_id: info.id,
                    value: val,
                });
            }
        }
    }

    fn save_current_patch(&mut self, name: &str) {
        let param_ids: Vec<u32> = self.param_infos.iter().map(|p| p.id).collect();
        let mode = match self.voice_mode_index {
            1 => VoiceMode::Mono,
            2 => VoiceMode::Unison {
                detune_cents: self.unison_detune,
            },
            _ => VoiceMode::Poly,
        };
        let patch = Patch::from_state(name, self.active_chip, mode, &param_ids, &self.param_values);
        if let Err(e) = self.patch_bank.save_patch(&patch) {
            log::error!("Failed to save patch: {}", e);
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
                GuiMessage::WaveformData { samples } => {
                    self.waveform_data = samples;
                }
            }
        }
    }

    // handle_computer_keyboard, handle_test_commands, execute_test_command,
    // dump_state are in input.rs
}

impl eframe::App for MameSynthApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.theme_applied {
            theme::apply_theme(ctx);
            self.theme_applied = true;
        }

        self.drain_gui_messages();
        self.handle_computer_keyboard(ctx);
        self.handle_test_commands(ctx);

        // Decay VU meters smoothly
        let decay = 0.85f32;
        self.peak_left *= decay;
        self.peak_right *= decay;

        // Poll MIDI player for events
        let was_playing = self.midi_player.is_playing();
        let midi_events = self.midi_player.poll();
        for event in midi_events {
            if event.is_on {
                let _ = self.audio_tx.push(AudioMessage::NoteOn {
                    note: event.note,
                    velocity: event.velocity,
                });
            } else {
                let _ = self
                    .audio_tx
                    .push(AudioMessage::NoteOff { note: event.note });
            }
        }
        // When playback ends, release all notes
        if was_playing && !self.midi_player.is_playing() {
            self.all_notes_off();
        }

        // Repaint at ~60fps for VU meters and MIDI playback progress
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

                ui.separator();
                let vpc = voices_per_chip(self.active_chip);
                let total = self.chip_count as u16 * vpc as u16;
                ui.label(format!("Chips: {}", self.chip_count));
                if ui.button("-").clicked() && self.chip_count > 1 {
                    self.chip_count -= 1;
                    let _ = self
                        .audio_tx
                        .push(AudioMessage::SetChipCount(self.chip_count));
                }
                if ui.button("+").clicked() && self.chip_count < 4 {
                    self.chip_count += 1;
                    let _ = self
                        .audio_tx
                        .push(AudioMessage::SetChipCount(self.chip_count));
                }
                ui.label(format!(
                    "({} × {} = {} voices)",
                    self.chip_count, vpc, total
                ));

                ui.separator();
                let macro_names = [
                    "Off",
                    "Pluck",
                    "Minor Arp",
                    "Major Arp",
                    "Octave Pulse",
                    "Kick Drum",
                    "Vibrato",
                    "Swell",
                ];
                let current_macro = if self.macro_index == 255 {
                    "Off"
                } else {
                    macro_names
                        .get(self.macro_index as usize + 1)
                        .unwrap_or(&"Off")
                };
                egui::ComboBox::from_id_salt("macro_select")
                    .selected_text(current_macro)
                    .width(100.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.macro_index == 255, "Off")
                            .clicked()
                        {
                            self.macro_index = 255;
                            let _ = self.audio_tx.push(AudioMessage::SetMacro(255));
                        }
                        for (i, name) in macro_names[1..].iter().enumerate() {
                            if ui
                                .selectable_label(self.macro_index == i as u8, *name)
                                .clicked()
                            {
                                self.macro_index = i as u8;
                                let _ = self.audio_tx.push(AudioMessage::SetMacro(i as u8));
                            }
                        }
                    });
            });
        });

        // Bottom panel: transport + keyboard
        egui::TopBottomPanel::bottom("keyboard_panel")
            .min_height(160.0)
            .show(ctx, |ui| {
                // MIDI transport bar
                crate::transport::show_transport(self, ui);
                ui.add_space(4.0);

                // Virtual keyboard
                let keyboard = PianoKeyboard::new(self.keyboard_octave, 4);
                let result = keyboard.show(ui, &self.held_keys, &mut self.mouse_note);

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
        // Save patch dialog (modal-ish)
        if self.show_save_dialog {
            egui::Window::new("Save Patch")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.save_patch_name);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() && !self.save_patch_name.is_empty() {
                            let name = self.save_patch_name.clone();
                            self.save_current_patch(&name);
                            self.show_save_dialog = false;
                            self.save_patch_name.clear();
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_save_dialog = false;
                        }
                    });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Chip selector + patch selector row
            ui.horizontal(|ui| {
                for chip in ChipId::all() {
                    let selected = *chip == self.active_chip;
                    let text = egui::RichText::new(chip.display_name()).size(14.0).strong();
                    if ui.selectable_label(selected, text).clicked() {
                        self.switch_chip(*chip);
                        self.selected_patch = None;
                    }
                }

                ui.separator();

                // Patch selector
                let patch_name = self
                    .selected_patch
                    .and_then(|i| self.patch_bank.list().get(i))
                    .map(|(name, _)| name.clone())
                    .unwrap_or_else(|| "(init)".into());
                let patches: Vec<(usize, String)> = self
                    .patch_bank
                    .list()
                    .iter()
                    .enumerate()
                    .map(|(i, (name, _))| (i, name.clone()))
                    .collect();
                let mut load_index: Option<usize> = None;
                egui::ComboBox::from_id_salt("patch_selector")
                    .selected_text(&patch_name)
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.selected_patch.is_none(), "(init)")
                            .clicked()
                        {
                            self.selected_patch = None;
                        }
                        for (i, name) in &patches {
                            if ui
                                .selectable_label(self.selected_patch == Some(*i), name.as_str())
                                .clicked()
                            {
                                load_index = Some(*i);
                            }
                        }
                    });
                // Apply patch load outside the combo closure
                if let Some(idx) = load_index {
                    self.selected_patch = Some(idx);
                    if let Some(patch) = self.patch_bank.load_patch(idx) {
                        self.load_patch(&patch);
                    }
                }

                if ui.button("Save").clicked() {
                    self.show_save_dialog = true;
                    self.save_patch_name = patch_name.to_string();
                    if self.save_patch_name == "(init)" {
                        self.save_patch_name.clear();
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
                ChipId::Ym2151 => {
                    panels::ym2612_panel::show_chip_header(ui); // reuse FM header for now
                    let algo_value = self.param_values.first().copied().unwrap_or(0.0);
                    panels::ym2612_panel::show_algorithm_diagram(ui, algo_value as u8);
                }
                _ => {
                    // Generic header for new chips
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(self.active_chip.display_name())
                                .strong()
                                .size(16.0),
                        );
                    });
                    ui.separator();
                }
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
