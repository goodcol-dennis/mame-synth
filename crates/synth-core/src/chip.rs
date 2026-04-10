/// Identifies a chip type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChipId {
    Sn76489,
    Ym2612,
    Sid6581,
    Ay8910,
    Ricoh2a03,
    Pokey,
    Ym2151,
    Ym3812,
    Ymf262,
    Scc,
    NamcoWsg,
}

impl ChipId {
    pub fn all() -> &'static [ChipId] {
        &[
            ChipId::Sn76489,
            ChipId::Ym2612,
            ChipId::Sid6581,
            ChipId::Ay8910,
            ChipId::Ricoh2a03,
            ChipId::Pokey,
            ChipId::Ym2151,
            ChipId::Ym3812,
            ChipId::Ymf262,
            ChipId::Scc,
            ChipId::NamcoWsg,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ChipId::Sn76489 => "SN76489 (PSG)",
            ChipId::Ym2612 => "YM2612 (FM)",
            ChipId::Sid6581 => "SID 6581 (C64)",
            ChipId::Ay8910 => "AY-3-8910 (PSG)",
            ChipId::Ricoh2a03 => "2A03 (NES)",
            ChipId::Pokey => "POKEY (Atari)",
            ChipId::Ym2151 => "YM2151 (OPM)",
            ChipId::Ym3812 => "YM3812 (OPL2)",
            ChipId::Ymf262 => "YMF262 (OPL3)",
            ChipId::Scc => "SCC (Konami)",
            ChipId::NamcoWsg => "Namco WSG",
        }
    }
}

/// Describes the type and range of a parameter for GUI rendering.
#[derive(Debug, Clone)]
pub enum ParamKind {
    /// Continuous float knob with [min, max] range.
    Continuous { min: f32, max: f32, default: f32 },
    /// Discrete integer selector (e.g., algorithm 0-7).
    Discrete {
        min: i32,
        max: i32,
        default: i32,
        labels: Option<Vec<String>>,
    },
    /// Boolean toggle.
    Toggle { default: bool },
}

impl ParamKind {
    pub fn default_value(&self) -> f32 {
        match self {
            ParamKind::Continuous { default, .. } => *default,
            ParamKind::Discrete { default, .. } => *default as f32,
            ParamKind::Toggle { default } => {
                if *default {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// Full metadata for one adjustable parameter.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    /// Unique ID within this chip (used in messages).
    pub id: u32,
    /// Human-readable name shown next to the knob/slider.
    pub name: String,
    /// Which group/section this belongs to (e.g., "Operator 1", "Noise").
    pub group: String,
    /// The kind and range of this parameter.
    pub kind: ParamKind,
}

/// Stereo sample pair.
#[derive(Debug, Clone, Copy, Default)]
pub struct StereoSample {
    pub left: f32,
    pub right: f32,
}

/// The core trait that every sound chip emulator implements.
/// Each chip exposes a fixed number of hardware voices.
pub trait SoundChip: Send {
    /// Which chip this is.
    fn chip_id(&self) -> ChipId;

    /// How many hardware voices this chip has.
    fn num_voices(&self) -> usize;

    /// Return metadata for all adjustable parameters.
    fn param_info(&self) -> Vec<ParamInfo>;

    /// Set a parameter by its id to a new value.
    fn set_param(&mut self, param_id: u32, value: f32);

    /// Get current value of a parameter.
    fn get_param(&self, param_id: u32) -> f32;

    /// Trigger a specific voice with a note.
    /// `detune_cents` offsets the pitch (for unison mode).
    fn voice_on(&mut self, voice: usize, note: u8, velocity: u8, detune_cents: f32);

    /// Release a specific voice.
    fn voice_off(&mut self, voice: usize);

    /// Generate stereo samples into the provided buffer.
    fn generate_samples(&mut self, output: &mut [StereoSample]);

    /// Reset all state to power-on defaults.
    fn reset(&mut self);
}

/// Voice allocation mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VoiceMode {
    Mono,
    Poly,
    Unison { detune_cents: f32 },
}

/// Get parameter info for a chip type without needing an instance.
pub fn param_info_for_chip(id: ChipId) -> Vec<ParamInfo> {
    match id {
        ChipId::Sn76489 => crate::sn76489::sn76489_param_info(),
        ChipId::Ym2612 => crate::ym2612::ym2612_param_info(),
        ChipId::Sid6581 => crate::sid6581::sid_param_info(),
        ChipId::Ay8910 => crate::ay8910::ay8910_param_info(),
        ChipId::Ricoh2a03 => crate::ricoh2a03::ricoh2a03_param_info(),
        ChipId::Pokey => crate::pokey::pokey_param_info(),
        ChipId::Ym2151 => crate::ym2151::ym2151_param_info(),
        ChipId::Ym3812 => crate::ym3812::ym3812_param_info(),
        ChipId::Ymf262 => crate::ymf262::ymf262_param_info(),
        ChipId::Scc => crate::scc::scc_param_info(),
        ChipId::NamcoWsg => crate::namco_wsg::namco_wsg_param_info(),
    }
}
