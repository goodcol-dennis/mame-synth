use crate::chip::{ChipId, VoiceMode};

/// Messages from GUI/MIDI thread to audio thread.
/// All variants are Copy for lock-free rtrb usage.
#[derive(Debug, Clone, Copy)]
pub enum AudioMessage {
    /// Change the active sound chip.
    SwitchChip(ChipId),
    /// Set a chip parameter.
    SetParam { param_id: u32, value: f32 },
    /// MIDI note on.
    NoteOn { note: u8, velocity: u8 },
    /// MIDI note off.
    NoteOff { note: u8 },
    /// Pitch bend (14-bit value, 8192 = center).
    PitchBend { value: u16 },
    /// Change voice allocation mode.
    SetVoiceMode(VoiceMode),
    /// Reset the active chip.
    Reset,
    /// Set number of chip instances in the active bank (1-4).
    SetChipCount(u8),
}

/// Messages from audio thread back to GUI (for visualization).
#[derive(Debug, Clone, Copy)]
pub enum GuiMessage {
    /// Current peak level (linear, 0.0-1.0) for VU meter.
    PeakLevel { left: f32, right: f32 },
}
