use eframe::egui;
use rtrb::{Consumer, Producer};
use synth_core::chip::{param_info_for_chip, ChipId, VoiceMode};
use synth_core::messages::{AudioMessage, GuiMessage};
use synth_core::midi::MidiHandler;
use synth_core::midi_file::{MidiPlayer, MidiSequence};
use synth_core::patch::{Patch, PatchBank};

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
    mouse_note: Option<u8>,
    patch_bank: PatchBank,
    selected_patch: Option<usize>,
    save_patch_name: String,
    show_save_dialog: bool,
    midi_player: MidiPlayer,
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
            mouse_note: None,
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

    /// F11: Read test command from /tmp/mame-synth-input.txt and execute it.
    /// F12: Dump current state to /tmp/mame-synth-state.txt.
    fn handle_test_commands(&mut self, ctx: &egui::Context) {
        let f11 = ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Key {
                        key: egui::Key::F11,
                        pressed: true,
                        ..
                    }
                )
            })
        });
        let f12 = ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Key {
                        key: egui::Key::F12,
                        pressed: true,
                        ..
                    }
                )
            })
        });

        if f11 {
            if let Ok(cmd) = std::fs::read_to_string("/tmp/mame-synth-input.txt") {
                self.execute_test_command(cmd.trim());
            }
        }

        if f12 {
            self.dump_state();
        }
    }

    fn execute_test_command(&mut self, cmd: &str) {
        log::info!("Test command: {}", cmd);
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        match parts.first().copied() {
            Some("switch-chip") => {
                if let Some(name) = parts.get(1) {
                    let chip = match *name {
                        "sn76489" => Some(ChipId::Sn76489),
                        "ym2612" => Some(ChipId::Ym2612),
                        "sid6581" => Some(ChipId::Sid6581),
                        _ => None,
                    };
                    if let Some(id) = chip {
                        self.switch_chip(id);
                    }
                }
            }
            Some("note-on") => {
                if let (Some(note), Some(vel)) = (
                    parts.get(1).and_then(|s| s.parse::<u8>().ok()),
                    parts.get(2).and_then(|s| s.parse::<u8>().ok()),
                ) {
                    self.held_keys.push(note);
                    let _ = self.audio_tx.push(AudioMessage::NoteOn {
                        note,
                        velocity: vel,
                    });
                }
            }
            Some("note-off") => {
                if let Some(note) = parts.get(1).and_then(|s| s.parse::<u8>().ok()) {
                    self.held_keys.retain(|&n| n != note);
                    let _ = self.audio_tx.push(AudioMessage::NoteOff { note });
                }
            }
            Some("set-param") => {
                if let (Some(id), Some(val)) = (
                    parts.get(1).and_then(|s| s.parse::<u32>().ok()),
                    parts.get(2).and_then(|s| s.parse::<f32>().ok()),
                ) {
                    let _ = self.audio_tx.push(AudioMessage::SetParam {
                        param_id: id,
                        value: val,
                    });
                    // Update local GUI state
                    if let Some(idx) = self.param_infos.iter().position(|p| p.id == id) {
                        self.param_values[idx] = val;
                    }
                }
            }
            Some("set-voice-mode") => {
                if let Some(mode_name) = parts.get(1) {
                    let mode = match *mode_name {
                        "mono" => Some(VoiceMode::Mono),
                        "poly" => Some(VoiceMode::Poly),
                        "unison" => {
                            let detune = parts
                                .get(2)
                                .and_then(|s| s.parse::<f32>().ok())
                                .unwrap_or(15.0);
                            Some(VoiceMode::Unison {
                                detune_cents: detune,
                            })
                        }
                        _ => None,
                    };
                    if let Some(m) = mode {
                        let _ = self.audio_tx.push(AudioMessage::SetVoiceMode(m));
                        match m {
                            VoiceMode::Mono => self.voice_mode_index = 1,
                            VoiceMode::Poly => self.voice_mode_index = 0,
                            VoiceMode::Unison { detune_cents } => {
                                self.voice_mode_index = 2;
                                self.unison_detune = detune_cents;
                            }
                        }
                    }
                }
            }
            Some("reset") => {
                let _ = self.audio_tx.push(AudioMessage::Reset);
            }
            _ => {
                log::warn!("Unknown test command: {}", cmd);
            }
        }
    }

    fn dump_state(&self) {
        let mut lines = Vec::new();
        lines.push(format!("chip={}", self.active_chip.display_name()));
        lines.push(format!("voice_mode={}", match self.voice_mode_index {
            0 => "poly",
            1 => "mono",
            2 => "unison",
            _ => "unknown",
        }));
        if self.voice_mode_index == 2 {
            lines.push(format!("unison_detune={:.1}", self.unison_detune));
        }
        lines.push(format!("octave={}", self.keyboard_octave));
        lines.push(format!("held_keys={}", self.held_keys.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",")));
        lines.push(format!("peak_left={:.4}", self.peak_left));
        lines.push(format!("peak_right={:.4}", self.peak_right));
        lines.push(format!("num_params={}", self.param_infos.len()));
        for (i, info) in self.param_infos.iter().enumerate() {
            lines.push(format!("param_{}={:.2}", info.id, self.param_values[i]));
        }
        let content = lines.join("\n");
        if let Err(e) = std::fs::write("/tmp/mame-synth-state.txt", &content) {
            log::error!("Failed to dump state: {}", e);
        }
        log::info!("State dumped to /tmp/mame-synth-state.txt");
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
        self.handle_test_commands(ctx);

        // Decay VU meters smoothly
        let decay = 0.85f32;
        self.peak_left *= decay;
        self.peak_right *= decay;

        // Poll MIDI player for events
        let midi_events = self.midi_player.poll();
        for event in midi_events {
            if event.is_on {
                let _ = self.audio_tx.push(AudioMessage::NoteOn {
                    note: event.note,
                    velocity: event.velocity,
                });
            } else {
                let _ = self.audio_tx.push(AudioMessage::NoteOff { note: event.note });
            }
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
            });
        });

        // Bottom panel: transport + keyboard
        egui::TopBottomPanel::bottom("keyboard_panel")
            .min_height(160.0)
            .show(ctx, |ui| {
                // MIDI transport bar
                ui.horizontal(|ui| {
                    if ui.button("Open MIDI").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("MIDI files", &["mid", "midi"])
                            .pick_file()
                        {
                            match MidiSequence::load(&path) {
                                Ok(seq) => {
                                    log::info!("Loaded MIDI: {} ({} events)", seq.name, seq.events.len());
                                    self.midi_player.load(seq);
                                }
                                Err(e) => log::error!("Failed to load MIDI: {}", e),
                            }
                        }
                    }

                    if self.midi_player.has_sequence() {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(self.midi_player.sequence_name())
                                .size(11.0)
                                .color(theme::ACCENT),
                        );

                        if self.midi_player.is_playing() {
                            if ui.button("Pause").clicked() {
                                self.midi_player.pause();
                            }
                        } else if ui.button("Play").clicked() {
                            self.midi_player.play();
                        }
                        if ui.button("Stop").clicked() {
                            self.midi_player.stop();
                        }

                        // Progress bar
                        let progress = self.midi_player.progress();
                        let pos_sec = self.midi_player.position_us() as f32 / 1_000_000.0;
                        let dur_sec = self.midi_player.duration_us() as f32 / 1_000_000.0;
                        ui.add(
                            egui::ProgressBar::new(progress)
                                .text(format!("{:.1}s / {:.1}s", pos_sec, dur_sec))
                                .desired_width(150.0),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(VuMeter::new(self.peak_right, "R"));
                        ui.add(VuMeter::new(self.peak_left, "L"));
                    });
                });
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
