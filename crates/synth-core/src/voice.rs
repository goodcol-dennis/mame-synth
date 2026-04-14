use crate::chip::{ChipId, ParamInfo, SoundChip, StereoSample, VoiceMode};
use crate::macros::{InstrumentMacro, MacroState};

/// Tracks which note each voice is playing and when it was triggered.
#[derive(Debug, Clone, Copy)]
struct VoiceSlot {
    note: Option<u8>,
    age: u64, // increments on each note_on, used for voice stealing
}

/// Allocates MIDI notes to chip voices based on the current VoiceMode.
pub struct VoiceAllocator {
    mode: VoiceMode,
    slots: Vec<VoiceSlot>,
    age_counter: u64,
    /// Mono mode: stack of held notes for last-note-priority
    note_stack: Vec<u8>,
}

impl VoiceAllocator {
    pub fn new(num_voices: usize) -> Self {
        VoiceAllocator {
            mode: VoiceMode::Poly,
            slots: vec![VoiceSlot { note: None, age: 0 }; num_voices],
            age_counter: 0,
            note_stack: Vec::new(),
        }
    }

    pub fn set_mode(&mut self, mode: VoiceMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> VoiceMode {
        self.mode
    }

    pub fn resize(&mut self, num_voices: usize) {
        self.slots
            .resize(num_voices, VoiceSlot { note: None, age: 0 });
    }

    /// Returns list of (voice_index, detune_cents) to trigger.
    pub fn note_on(&mut self, note: u8) -> Vec<(usize, f32)> {
        self.age_counter += 1;
        match self.mode {
            VoiceMode::Mono => {
                self.note_stack.retain(|&n| n != note);
                self.note_stack.push(note);
                // Always use voice 0
                self.slots[0] = VoiceSlot {
                    note: Some(note),
                    age: self.age_counter,
                };
                vec![(0, 0.0)]
            }
            VoiceMode::Poly => {
                let voice = self.find_free_or_steal();
                self.slots[voice] = VoiceSlot {
                    note: Some(note),
                    age: self.age_counter,
                };
                vec![(voice, 0.0)]
            }
            VoiceMode::Unison { detune_cents } => {
                let n = self.slots.len();
                let mut result = Vec::with_capacity(n);
                for i in 0..n {
                    // Spread detune evenly: -detune ... 0 ... +detune
                    let detune = if n == 1 {
                        0.0
                    } else {
                        let t = i as f32 / (n - 1) as f32; // 0.0 to 1.0
                        (t - 0.5) * 2.0 * detune_cents
                    };
                    self.slots[i] = VoiceSlot {
                        note: Some(note),
                        age: self.age_counter,
                    };
                    result.push((i, detune));
                }
                result
            }
        }
    }

    /// Returns list of voice indices to release.
    pub fn note_off(&mut self, note: u8) -> Vec<usize> {
        match self.mode {
            VoiceMode::Mono => {
                self.note_stack.retain(|&n| n != note);
                if let Some(&prev_note) = self.note_stack.last() {
                    // Retrigger previous note in the stack
                    self.slots[0].note = Some(prev_note);
                    // Return empty — caller should retrigger voice 0 with prev_note
                    // We signal this with a special convention: empty vec means "retrigger"
                    // Actually let's return the voice to release, and handle retrigger separately
                    vec![] // don't release — we'll retrigger in the caller
                } else {
                    self.slots[0].note = None;
                    vec![0]
                }
            }
            VoiceMode::Poly => {
                let mut released = Vec::new();
                for (i, slot) in self.slots.iter_mut().enumerate() {
                    if slot.note == Some(note) {
                        slot.note = None;
                        released.push(i);
                    }
                }
                released
            }
            VoiceMode::Unison { .. } => {
                // Release all voices if they're playing this note
                let mut released = Vec::new();
                for (i, slot) in self.slots.iter_mut().enumerate() {
                    if slot.note == Some(note) {
                        slot.note = None;
                        released.push(i);
                    }
                }
                released
            }
        }
    }

    /// For mono mode: get the note that should be retriggered after a note_off.
    pub fn mono_retrigger_note(&self) -> Option<u8> {
        if matches!(self.mode, VoiceMode::Mono) {
            self.note_stack.last().copied()
        } else {
            None
        }
    }

    fn find_free_or_steal(&self) -> usize {
        // First: find a free voice
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.note.is_none() {
                return i;
            }
        }
        // All busy: steal the oldest (lowest age)
        self.slots
            .iter()
            .enumerate()
            .min_by_key(|(_, s)| s.age)
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
}

/// Wraps one or more instances of the same chip type, presenting them
/// as a single instrument with a pooled voice count.
pub struct ChipBank {
    chips: Vec<Box<dyn SoundChip>>,
    voices_per_chip: usize,
    allocator: VoiceAllocator,
    mix_buffer: Vec<StereoSample>,
    // Macro engine
    active_macro: Option<InstrumentMacro>,
    macro_states: Vec<MacroState>,
    voice_base_notes: Vec<Option<u8>>,
    voice_velocities: Vec<u8>,
    voice_detunes: Vec<f32>,
    macro_sample_counter: u32,
    macro_tick_interval: u32, // samples per macro tick (~735 for 60Hz)
}

impl ChipBank {
    pub fn new(chips: Vec<Box<dyn SoundChip>>) -> Self {
        assert!(!chips.is_empty());
        let voices_per_chip = chips[0].num_voices();
        let total_voices = voices_per_chip * chips.len();
        ChipBank {
            chips,
            voices_per_chip,
            allocator: VoiceAllocator::new(total_voices),
            mix_buffer: Vec::new(),
            active_macro: None,
            macro_states: vec![MacroState::new(); total_voices],
            voice_base_notes: vec![None; total_voices],
            voice_velocities: vec![0; total_voices],
            voice_detunes: vec![0.0; total_voices],
            macro_sample_counter: 0,
            macro_tick_interval: 735, // 44100 / 60 ≈ 735
        }
    }

    pub fn chip_id(&self) -> ChipId {
        self.chips[0].chip_id()
    }

    pub fn total_voices(&self) -> usize {
        self.voices_per_chip * self.chips.len()
    }

    pub fn num_chips(&self) -> usize {
        self.chips.len()
    }

    pub fn set_voice_mode(&mut self, mode: VoiceMode) {
        self.allocator.set_mode(mode);
    }

    pub fn voice_mode(&self) -> VoiceMode {
        self.allocator.mode()
    }

    pub fn set_macro(&mut self, mac: Option<InstrumentMacro>) {
        self.active_macro = mac;
        // Reset all macro states
        for ms in &mut self.macro_states {
            *ms = MacroState::new();
        }
    }

    pub fn active_macro_name(&self) -> Option<&str> {
        self.active_macro.as_ref().map(|m| m.name.as_str())
    }

    pub fn param_info(&self) -> Vec<ParamInfo> {
        self.chips[0].param_info()
    }

    pub fn set_param(&mut self, param_id: u32, value: f32) {
        // Apply to all chip instances
        for chip in &mut self.chips {
            chip.set_param(param_id, value);
        }
    }

    pub fn get_param(&self, param_id: u32) -> f32 {
        self.chips[0].get_param(param_id)
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        let triggers = self.allocator.note_on(note);
        for (voice_idx, detune) in triggers {
            let chip_idx = voice_idx / self.voices_per_chip;
            let local_voice = voice_idx % self.voices_per_chip;
            self.chips[chip_idx].voice_on(local_voice, note, velocity, detune);
            // Track for macro modulation
            if voice_idx < self.voice_base_notes.len() {
                self.voice_base_notes[voice_idx] = Some(note);
                self.voice_velocities[voice_idx] = velocity;
                self.voice_detunes[voice_idx] = detune;
                self.macro_states[voice_idx].trigger();
            }
        }
    }

    pub fn note_off(&mut self, note: u8) {
        let releases = self.allocator.note_off(note);
        for voice_idx in &releases {
            let chip_idx = voice_idx / self.voices_per_chip;
            let local_voice = voice_idx % self.voices_per_chip;
            self.chips[chip_idx].voice_off(local_voice);
            if *voice_idx < self.macro_states.len() {
                self.macro_states[*voice_idx].release();
                self.voice_base_notes[*voice_idx] = None;
            }
        }

        // Mono retrigger
        if releases.is_empty() {
            if let Some(retrigger_note) = self.allocator.mono_retrigger_note() {
                self.chips[0].voice_on(0, retrigger_note, 100, 0.0);
                if !self.voice_base_notes.is_empty() {
                    self.voice_base_notes[0] = Some(retrigger_note);
                    self.macro_states[0].trigger();
                }
            }
        }
    }

    pub fn generate_samples(&mut self, output: &mut [StereoSample]) {
        // Tick macros at frame rate (~60Hz) and apply modulation
        if let Some(mac) = &self.active_macro {
            let mac = mac.clone(); // clone to avoid borrow conflict
            self.macro_sample_counter += output.len() as u32;
            while self.macro_sample_counter >= self.macro_tick_interval {
                self.macro_sample_counter -= self.macro_tick_interval;
                self.apply_macro_tick(&mac);
            }
        }

        // First chip writes directly
        self.chips[0].generate_samples(output);

        if self.chips.len() > 1 {
            self.mix_buffer
                .resize(output.len(), StereoSample::default());
            for chip in &mut self.chips[1..] {
                chip.generate_samples(&mut self.mix_buffer);
                for (out, mix) in output.iter_mut().zip(self.mix_buffer.iter()) {
                    out.left += mix.left;
                    out.right += mix.right;
                }
            }
            let scale = 1.0 / self.chips.len() as f32;
            for s in output.iter_mut() {
                s.left *= scale;
                s.right *= scale;
            }
        }
    }

    fn apply_macro_tick(&mut self, mac: &InstrumentMacro) {
        let total_voices = self.voices_per_chip * self.chips.len();
        for voice_idx in 0..total_voices {
            if self.voice_base_notes[voice_idx].is_none() {
                continue;
            }
            let out = self.macro_states[voice_idx].tick(mac);
            let base_note = self.voice_base_notes[voice_idx].unwrap();
            let velocity = self.voice_velocities[voice_idx];
            let detune = self.voice_detunes[voice_idx];

            let chip_idx = voice_idx / self.voices_per_chip;
            let local_voice = voice_idx % self.voices_per_chip;

            // Apply arpeggio: retrigger voice with offset note
            if let Some(arp) = out.arp_semitones {
                let new_note = (base_note as i16 + arp as i16).clamp(0, 127) as u8;
                self.chips[chip_idx].voice_on(local_voice, new_note, velocity, detune);
            }

            // Apply volume modulation (for chips that support per-voice volume)
            // This is chip-specific — for PSGs, volume is the primary control
            if let Some(_vol) = out.volume {
                // TODO: apply volume to chip (needs per-voice volume API)
                // For now, macros with volume sequences use arpeggio only
            }
        }
    }

    pub fn reset(&mut self) {
        for chip in &mut self.chips {
            chip.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_allocator(voices: usize) -> VoiceAllocator {
        VoiceAllocator::new(voices)
    }

    // --- Poly mode ---

    #[test]
    fn poly_assigns_different_voices() {
        let mut alloc = make_allocator(3);
        let v1 = alloc.note_on(60);
        let v2 = alloc.note_on(64);
        let v3 = alloc.note_on(67);
        assert_eq!(v1.len(), 1);
        assert_eq!(v2.len(), 1);
        assert_eq!(v3.len(), 1);
        assert_ne!(v1[0].0, v2[0].0);
        assert_ne!(v2[0].0, v3[0].0);
    }

    #[test]
    fn poly_steals_oldest_when_full() {
        let mut alloc = make_allocator(2);
        let v1 = alloc.note_on(60);
        let _v2 = alloc.note_on(64);
        let v3 = alloc.note_on(67); // should steal v1's voice
        assert_eq!(v3[0].0, v1[0].0);
    }

    #[test]
    fn poly_reuses_released_voice() {
        let mut alloc = make_allocator(2);
        alloc.note_on(60);
        alloc.note_on(64);
        alloc.note_off(60); // free voice 0
        let v3 = alloc.note_on(67);
        assert_eq!(v3[0].0, 0); // should reuse voice 0
    }

    #[test]
    fn poly_note_off_returns_correct_voice() {
        let mut alloc = make_allocator(3);
        alloc.note_on(60);
        alloc.note_on(64);
        alloc.note_on(67);
        let released = alloc.note_off(64);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0], 1);
    }

    // --- Mono mode ---

    #[test]
    fn mono_always_uses_voice_0() {
        let mut alloc = make_allocator(3);
        alloc.set_mode(VoiceMode::Mono);
        let v1 = alloc.note_on(60);
        let v2 = alloc.note_on(64);
        assert_eq!(v1[0].0, 0);
        assert_eq!(v2[0].0, 0);
    }

    #[test]
    fn mono_retrigger_on_release() {
        let mut alloc = make_allocator(3);
        alloc.set_mode(VoiceMode::Mono);
        alloc.note_on(60);
        alloc.note_on(64);
        let released = alloc.note_off(64);
        assert!(released.is_empty()); // no release — retrigger instead
        assert_eq!(alloc.mono_retrigger_note(), Some(60));
    }

    #[test]
    fn mono_releases_when_stack_empty() {
        let mut alloc = make_allocator(3);
        alloc.set_mode(VoiceMode::Mono);
        alloc.note_on(60);
        let released = alloc.note_off(60);
        assert_eq!(released, vec![0]);
        assert_eq!(alloc.mono_retrigger_note(), None);
    }

    // --- Unison mode ---

    #[test]
    fn unison_triggers_all_voices() {
        let mut alloc = make_allocator(3);
        alloc.set_mode(VoiceMode::Unison { detune_cents: 10.0 });
        let voices = alloc.note_on(60);
        assert_eq!(voices.len(), 3);
    }

    #[test]
    fn unison_detune_spread_symmetric() {
        let mut alloc = make_allocator(3);
        alloc.set_mode(VoiceMode::Unison { detune_cents: 10.0 });
        let voices = alloc.note_on(60);
        let detunes: Vec<f32> = voices.iter().map(|(_, d)| *d).collect();
        assert!((detunes[0] - (-10.0)).abs() < 0.01);
        assert!((detunes[1] - 0.0).abs() < 0.01);
        assert!((detunes[2] - 10.0).abs() < 0.01);
    }

    #[test]
    fn unison_releases_all_voices() {
        let mut alloc = make_allocator(3);
        alloc.set_mode(VoiceMode::Unison { detune_cents: 10.0 });
        alloc.note_on(60);
        let released = alloc.note_off(60);
        assert_eq!(released.len(), 3);
    }
}
