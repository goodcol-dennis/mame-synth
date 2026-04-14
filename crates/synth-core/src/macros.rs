//! Instrument macro engine.
//!
//! PSG chips (SN76489, AY-3-8910, etc.) are simple square/noise generators.
//! Real game music makes them sound alive by rapidly changing registers at
//! the frame rate (50/60 Hz). This module provides "instrument macros" —
//! timed sequences of register modifications that run per-voice.
//!
//! Macros modulate: volume, pitch (arpeggio), and duty/waveform.

use serde::{Deserialize, Serialize};

/// A single instrument macro with volume, pitch, and duty sequences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentMacro {
    pub name: String,
    /// Volume sequence (0-15 per step). Empty = no volume macro.
    pub volume: Vec<u8>,
    /// Pitch offset in semitones per step (for arpeggio). Empty = no arpeggio.
    pub arpeggio: Vec<i8>,
    /// Duty/waveform index per step. Empty = no duty macro.
    pub duty: Vec<u8>,
    /// Step at which the sequence loops (None = play once, hold last value).
    pub loop_point: Option<usize>,
    /// Ticks per step (1 = every frame, 2 = every other frame, etc.)
    pub speed: u8,
}

impl Default for InstrumentMacro {
    fn default() -> Self {
        InstrumentMacro {
            name: "Default".into(),
            volume: Vec::new(),
            arpeggio: Vec::new(),
            duty: Vec::new(),
            loop_point: None,
            speed: 1,
        }
    }
}

impl InstrumentMacro {
    pub fn is_empty(&self) -> bool {
        self.volume.is_empty() && self.arpeggio.is_empty() && self.duty.is_empty()
    }
}

/// Per-voice macro playback state.
#[derive(Debug, Clone)]
pub struct MacroState {
    pub active: bool,
    tick: u32,
    vol_pos: usize,
    arp_pos: usize,
    duty_pos: usize,
}

impl Default for MacroState {
    fn default() -> Self {
        Self::new()
    }
}

impl MacroState {
    pub fn new() -> Self {
        MacroState {
            active: false,
            tick: 0,
            vol_pos: 0,
            arp_pos: 0,
            duty_pos: 0,
        }
    }

    pub fn trigger(&mut self) {
        self.active = true;
        self.tick = 0;
        self.vol_pos = 0;
        self.arp_pos = 0;
        self.duty_pos = 0;
    }

    pub fn release(&mut self) {
        self.active = false;
    }

    /// Advance one tick and return current modulation values.
    pub fn tick(&mut self, mac: &InstrumentMacro) -> MacroOutput {
        if !self.active || mac.is_empty() {
            return MacroOutput::default();
        }

        let step = if mac.speed > 0 {
            (self.tick / mac.speed as u32) as usize
        } else {
            self.tick as usize
        };
        self.tick += 1;

        MacroOutput {
            volume: get_seq_value(&mac.volume, step, mac.loop_point),
            arp_semitones: get_seq_value_i8(&mac.arpeggio, step, mac.loop_point),
            duty: get_seq_value(&mac.duty, step, mac.loop_point),
        }
    }
}

/// Current output from the macro engine for one voice.
#[derive(Debug, Clone, Copy, Default)]
pub struct MacroOutput {
    /// Volume override (None = use default)
    pub volume: Option<u8>,
    /// Pitch offset in semitones (None = no arpeggio)
    pub arp_semitones: Option<i8>,
    /// Duty/waveform index (None = use default)
    pub duty: Option<u8>,
}

fn get_seq_value(seq: &[u8], step: usize, loop_point: Option<usize>) -> Option<u8> {
    if seq.is_empty() {
        return None;
    }
    let idx = if step < seq.len() {
        step
    } else if let Some(lp) = loop_point {
        if lp < seq.len() {
            lp + (step - seq.len()) % (seq.len() - lp)
        } else {
            seq.len() - 1
        }
    } else {
        seq.len() - 1 // hold last value
    };
    Some(seq[idx])
}

fn get_seq_value_i8(seq: &[i8], step: usize, loop_point: Option<usize>) -> Option<i8> {
    if seq.is_empty() {
        return None;
    }
    let idx = if step < seq.len() {
        step
    } else if let Some(lp) = loop_point {
        if lp < seq.len() {
            lp + (step - seq.len()) % (seq.len() - lp)
        } else {
            seq.len() - 1
        }
    } else {
        seq.len() - 1
    };
    Some(seq[idx])
}

// ─── Factory presets ─────────────────────────────────────────────────

pub fn factory_macros() -> Vec<InstrumentMacro> {
    vec![
        InstrumentMacro {
            name: "Pluck".into(),
            volume: vec![15, 13, 11, 9, 7, 5, 4, 3, 2, 1, 0],
            arpeggio: Vec::new(),
            duty: Vec::new(),
            loop_point: None,
            speed: 1,
        },
        InstrumentMacro {
            name: "Minor Arp".into(),
            volume: Vec::new(),
            arpeggio: vec![0, 3, 7, 12],
            duty: Vec::new(),
            loop_point: Some(0),
            speed: 2,
        },
        InstrumentMacro {
            name: "Major Arp".into(),
            volume: Vec::new(),
            arpeggio: vec![0, 4, 7, 12],
            duty: Vec::new(),
            loop_point: Some(0),
            speed: 2,
        },
        InstrumentMacro {
            name: "Octave Pulse".into(),
            volume: Vec::new(),
            arpeggio: vec![0, 12],
            duty: Vec::new(),
            loop_point: Some(0),
            speed: 1,
        },
        InstrumentMacro {
            name: "Kick Drum".into(),
            volume: vec![15, 14, 12, 8, 4, 2, 1, 0],
            arpeggio: vec![0, -12, -24, -36],
            duty: Vec::new(),
            loop_point: None,
            speed: 1,
        },
        InstrumentMacro {
            name: "Vibrato".into(),
            volume: Vec::new(),
            arpeggio: vec![0, 0, 0, 0, 1, 0, 0, 0, 0, -1],
            duty: Vec::new(),
            loop_point: Some(0),
            speed: 1,
        },
        InstrumentMacro {
            name: "Swell".into(),
            volume: vec![
                0, 1, 2, 3, 5, 7, 9, 11, 13, 15, 15, 15, 15, 13, 11, 9, 7, 5, 3, 1,
            ],
            arpeggio: Vec::new(),
            duty: Vec::new(),
            loop_point: Some(0),
            speed: 1,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_macro_no_output() {
        let mac = InstrumentMacro::default();
        let mut state = MacroState::new();
        state.trigger();
        let out = state.tick(&mac);
        assert!(out.volume.is_none());
        assert!(out.arp_semitones.is_none());
    }

    #[test]
    fn volume_decay() {
        let mac = InstrumentMacro {
            volume: vec![15, 10, 5, 0],
            ..Default::default()
        };
        let mut state = MacroState::new();
        state.trigger();
        assert_eq!(state.tick(&mac).volume, Some(15));
        assert_eq!(state.tick(&mac).volume, Some(10));
        assert_eq!(state.tick(&mac).volume, Some(5));
        assert_eq!(state.tick(&mac).volume, Some(0));
        // Past end: hold last
        assert_eq!(state.tick(&mac).volume, Some(0));
    }

    #[test]
    fn arpeggio_loops() {
        let mac = InstrumentMacro {
            arpeggio: vec![0, 4, 7],
            loop_point: Some(0),
            ..Default::default()
        };
        let mut state = MacroState::new();
        state.trigger();
        assert_eq!(state.tick(&mac).arp_semitones, Some(0));
        assert_eq!(state.tick(&mac).arp_semitones, Some(4));
        assert_eq!(state.tick(&mac).arp_semitones, Some(7));
        // Loop back to 0
        assert_eq!(state.tick(&mac).arp_semitones, Some(0));
        assert_eq!(state.tick(&mac).arp_semitones, Some(4));
    }

    #[test]
    fn speed_divider() {
        let mac = InstrumentMacro {
            arpeggio: vec![0, 12],
            loop_point: Some(0),
            speed: 3,
            ..Default::default()
        };
        let mut state = MacroState::new();
        state.trigger();
        // Speed 3: same value for 3 ticks
        assert_eq!(state.tick(&mac).arp_semitones, Some(0));
        assert_eq!(state.tick(&mac).arp_semitones, Some(0));
        assert_eq!(state.tick(&mac).arp_semitones, Some(0));
        // Now advances
        assert_eq!(state.tick(&mac).arp_semitones, Some(12));
    }

    #[test]
    fn release_stops_output() {
        let mac = InstrumentMacro {
            volume: vec![15, 10, 5],
            ..Default::default()
        };
        let mut state = MacroState::new();
        state.trigger();
        assert_eq!(state.tick(&mac).volume, Some(15));
        state.release();
        assert!(state.tick(&mac).volume.is_none());
    }
}
