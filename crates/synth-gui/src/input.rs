use eframe::egui;
use synth_core::chip::{ChipId, VoiceMode};
use synth_core::messages::AudioMessage;

use crate::app::MameSynthApp;

/// Computer keyboard → MIDI note mapping.
const KEY_MAP: &[(egui::Key, u8)] = &[
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

impl MameSynthApp {
    pub(crate) fn handle_computer_keyboard(&mut self, ctx: &egui::Context) {
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
                    for (mapped_key, semitone) in KEY_MAP {
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
                let also_pressed = raw_events
                    .iter()
                    .any(|&(n, p, r)| n == midi_note && p && !r);
                if also_pressed {
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

    /// Poll /tmp/mame-synth-input.txt every frame for test commands.
    ///
    /// Writing a command to that file is sufficient — no key injection needed.
    /// Supported commands are the same as before plus `dump-state` (which
    /// writes /tmp/mame-synth-state.txt).  The file is deleted after it is
    /// consumed so each command fires exactly once.
    pub(crate) fn handle_test_commands(&mut self, _ctx: &egui::Context) {
        let input_path = "/tmp/mame-synth-input.txt";
        if let Ok(cmd) = std::fs::read_to_string(input_path) {
            // Remove the file first so a slow frame cannot re-trigger it.
            let _ = std::fs::remove_file(input_path);
            let cmd = cmd.trim().to_string();
            if cmd == "dump-state" {
                self.dump_state();
            } else if !cmd.is_empty() {
                self.execute_test_command(&cmd);
            }
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
                        "ay8910" => Some(ChipId::Ay8910),
                        "2a03" => Some(ChipId::Ricoh2a03),
                        "pokey" => Some(ChipId::Pokey),
                        "ym2151" => Some(ChipId::Ym2151),
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
        lines.push(format!(
            "voice_mode={}",
            match self.voice_mode_index {
                0 => "poly",
                1 => "mono",
                2 => "unison",
                _ => "unknown",
            }
        ));
        if self.voice_mode_index == 2 {
            lines.push(format!("unison_detune={:.1}", self.unison_detune));
        }
        lines.push(format!("octave={}", self.keyboard_octave));
        lines.push(format!(
            "held_keys={}",
            self.held_keys
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ));
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
