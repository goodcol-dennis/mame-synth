use egui::{self, Sense};

use crate::theme;

const WHITE_KEYS_PER_OCTAVE: usize = 7;

const IS_BLACK: [bool; 12] = [
    false, true, false, true, false, false, true, false, true, false, true, false,
];

pub struct PianoKeyboard {
    pub base_octave: u8,
    pub num_octaves: u8,
}

pub struct KeyboardResult {
    pub note_on: Option<u8>,
    pub note_off: Option<u8>,
}

struct KeyRect {
    rect: egui::Rect,
    midi_note: u8,
    is_black: bool,
}

impl PianoKeyboard {
    pub fn new(base_octave: u8, num_octaves: u8) -> Self {
        PianoKeyboard {
            base_octave,
            num_octaves,
        }
    }

    pub fn show(
        &self,
        ui: &mut egui::Ui,
        held_keys: &[u8],
        mouse_note: &mut Option<u8>,
    ) -> KeyboardResult {
        let mut result = KeyboardResult {
            note_on: None,
            note_off: None,
        };

        let total_white = self.num_octaves as usize * WHITE_KEYS_PER_OCTAVE;
        let available_width = ui.available_width();
        let key_width = (available_width / total_white as f32).min(28.0);
        let white_height = 120.0;
        let black_height = 72.0;
        let black_width = key_width * 0.65;

        let total_width = key_width * total_white as f32;
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(total_width, white_height), Sense::click_and_drag());

        if !ui.is_rect_visible(rect) {
            return result;
        }

        let painter = ui.painter_at(rect);
        let keys = self.build_key_rects(rect, key_width, white_height, black_height, black_width);

        // Hit test: find which key the pointer is over
        let hit_note = response
            .interact_pointer_pos()
            .and_then(|pos| self.hit_test(&keys, pos));

        // Mouse interaction state machine:
        // - Pointer down on a key → note_on
        // - Pointer dragged to a different key → note_off old, note_on new (glissando)
        // - Pointer released or left the keyboard → note_off
        let pointer_down = response.is_pointer_button_down_on();

        if pointer_down {
            if let Some(note) = hit_note {
                if *mouse_note != Some(note) {
                    // Release previous mouse note if any
                    if let Some(prev) = *mouse_note {
                        result.note_off = Some(prev);
                    }
                    // Press new note
                    result.note_on = Some(note);
                    *mouse_note = Some(note);
                }
            }
        } else if mouse_note.is_some() {
            // Pointer released — release the mouse note
            result.note_off = *mouse_note;
            *mouse_note = None;
        }

        // Draw white keys
        for key in &keys {
            if key.is_black {
                continue;
            }
            let is_pressed = held_keys.contains(&key.midi_note);
            let fill = if is_pressed {
                theme::KEY_PRESSED
            } else {
                theme::KEY_WHITE
            };
            painter.rect_filled(key.rect.shrink(0.5), 2.0, fill);
            painter.rect_stroke(
                key.rect.shrink(0.5),
                2.0,
                egui::Stroke::new(1.0, egui::Color32::from_gray(100)),
                egui::StrokeKind::Outside,
            );
        }

        // Draw black keys on top
        for key in &keys {
            if !key.is_black {
                continue;
            }
            let is_pressed = held_keys.contains(&key.midi_note);
            let fill = if is_pressed {
                theme::KEY_PRESSED
            } else {
                theme::KEY_BLACK
            };
            painter.rect_filled(key.rect, 2.0, fill);
        }

        result
    }

    fn build_key_rects(
        &self,
        rect: egui::Rect,
        key_width: f32,
        white_height: f32,
        black_height: f32,
        black_width: f32,
    ) -> Vec<KeyRect> {
        let mut keys = Vec::new();

        // White keys
        let mut white_idx = 0;
        for octave in 0..self.num_octaves {
            for (semitone, &is_black) in IS_BLACK.iter().enumerate() {
                if is_black {
                    continue;
                }
                let x = rect.left() + white_idx as f32 * key_width;
                let midi_note = (self.base_octave + octave) * 12 + semitone as u8;
                keys.push(KeyRect {
                    rect: egui::Rect::from_min_size(
                        egui::pos2(x, rect.top()),
                        egui::vec2(key_width, white_height),
                    ),
                    midi_note,
                    is_black: false,
                });
                white_idx += 1;
            }
        }

        // Black keys (appended after white so they're checked first in reverse)
        white_idx = 0;
        for octave in 0..self.num_octaves {
            for (semitone, &is_black) in IS_BLACK.iter().enumerate() {
                if is_black {
                    let x = rect.left() + white_idx as f32 * key_width - black_width / 2.0;
                    let midi_note = (self.base_octave + octave) * 12 + semitone as u8;
                    keys.push(KeyRect {
                        rect: egui::Rect::from_min_size(
                            egui::pos2(x, rect.top()),
                            egui::vec2(black_width, black_height),
                        ),
                        midi_note,
                        is_black: true,
                    });
                } else {
                    white_idx += 1;
                }
            }
        }

        keys
    }

    fn hit_test(&self, keys: &[KeyRect], pos: egui::Pos2) -> Option<u8> {
        // Check black keys first (they're at the end and drawn on top)
        for key in keys.iter().rev() {
            if key.rect.contains(pos) {
                return Some(key.midi_note);
            }
        }
        None
    }
}
