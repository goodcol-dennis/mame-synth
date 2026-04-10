use egui::{self, Sense};

use crate::theme;

const WHITE_KEYS_PER_OCTAVE: usize = 7;
const KEYS_PER_OCTAVE: usize = 12;

// Which notes in an octave are black keys
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
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(total_width, white_height),
            Sense::click_and_drag(),
        );

        if !ui.is_rect_visible(rect) {
            return result;
        }

        let painter = ui.painter_at(rect);

        // Build key rectangles for hit testing
        struct KeyRect {
            rect: egui::Rect,
            midi_note: u8,
            is_black: bool,
        }
        let mut keys = Vec::new();

        // White keys first
        let mut white_idx = 0;
        for octave in 0..self.num_octaves {
            for semitone in 0..KEYS_PER_OCTAVE {
                if IS_BLACK[semitone] {
                    continue;
                }
                let x = rect.left() + white_idx as f32 * key_width;
                let midi_note =
                    (self.base_octave + octave) * 12 + semitone as u8;
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

        // Black keys (drawn on top)
        white_idx = 0;
        for octave in 0..self.num_octaves {
            for semitone in 0..KEYS_PER_OCTAVE {
                if IS_BLACK[semitone] {
                    // Black keys are positioned between white keys
                    let x = rect.left() + white_idx as f32 * key_width - black_width / 2.0;
                    let midi_note =
                        (self.base_octave + octave) * 12 + semitone as u8;
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

        // Hit test: check pointer position
        if let Some(pos) = response.interact_pointer_pos() {
            // Check black keys first (they're on top)
            let mut hit_note = None;
            for key in keys.iter().rev() {
                if key.rect.contains(pos) {
                    hit_note = Some(key.midi_note);
                    break;
                }
            }

            if response.drag_started() {
                if let Some(note) = hit_note {
                    result.note_on = Some(note);
                }
            } else if response.drag_stopped() {
                if let Some(note) = hit_note {
                    result.note_off = Some(note);
                }
            }
        }

        if response.drag_stopped() && result.note_off.is_none() {
            // Release whatever was held from this keyboard
            // The app will handle this based on held_keys
            if let Some(&last) = held_keys.last() {
                result.note_off = Some(last);
            }
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
            painter.rect_stroke(key.rect.shrink(0.5), 2.0, egui::Stroke::new(1.0, egui::Color32::from_gray(100)), egui::StrokeKind::Outside);
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
}
