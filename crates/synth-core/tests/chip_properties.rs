use proptest::prelude::*;
use synth_core::chip::{SoundChip, StereoSample};
use synth_core::sid6581::Sid6581;
use synth_core::sn76489::Sn76489;

const SAMPLE_RATE: u32 = 44100;
const BUF_SIZE: usize = 1024;

fn gen_samples(chip: &mut dyn SoundChip, n: usize) -> Vec<StereoSample> {
    let mut buf = vec![StereoSample::default(); n];
    chip.generate_samples(&mut buf);
    buf
}

fn no_nan_or_inf(buf: &[StereoSample]) -> bool {
    buf.iter()
        .all(|s| s.left.is_finite() && s.right.is_finite())
}

fn no_clipping(buf: &[StereoSample]) -> bool {
    buf.iter()
        .all(|s| s.left.abs() <= 2.0 && s.right.abs() <= 2.0)
}

// --- SN76489 properties ---

proptest! {
    #[test]
    fn sn76489_never_nan(note in 21u8..108, vel in 1u8..127, voice in 0usize..3) {
        let mut chip = Sn76489::new(SAMPLE_RATE);
        chip.voice_on(voice, note, vel, 0.0);
        let buf = gen_samples(&mut chip, BUF_SIZE);
        prop_assert!(no_nan_or_inf(&buf));
    }

    #[test]
    fn sn76489_never_clips(note in 21u8..108, vel in 1u8..127) {
        let mut chip = Sn76489::new(SAMPLE_RATE);
        // All 3 voices at once
        chip.voice_on(0, note, vel, 0.0);
        chip.voice_on(1, note.saturating_add(4).min(107), vel, 0.0);
        chip.voice_on(2, note.saturating_add(7).min(107), vel, 0.0);
        let buf = gen_samples(&mut chip, BUF_SIZE);
        prop_assert!(no_clipping(&buf));
    }

    #[test]
    fn sn76489_silent_after_reset(note in 21u8..108, vel in 1u8..127) {
        let mut chip = Sn76489::new(SAMPLE_RATE);
        chip.voice_on(0, note, vel, 0.0);
        gen_samples(&mut chip, 256);
        chip.reset();
        let buf = gen_samples(&mut chip, 256);
        prop_assert!(buf.iter().all(|s| s.left == 0.0 && s.right == 0.0));
    }

    #[test]
    fn sn76489_detune_valid_range(note in 21u8..108, detune in -100.0f32..100.0) {
        let mut chip = Sn76489::new(SAMPLE_RATE);
        chip.voice_on(0, note, 100, detune);
        let buf = gen_samples(&mut chip, BUF_SIZE);
        prop_assert!(no_nan_or_inf(&buf));
        prop_assert!(no_clipping(&buf));
    }
}

// --- SID properties ---

proptest! {
    #[test]
    fn sid_never_nan(note in 21u8..108, vel in 1u8..127, voice in 0usize..3) {
        let mut chip = Sid6581::new(SAMPLE_RATE);
        chip.voice_on(voice, note, vel, 0.0);
        let buf = gen_samples(&mut chip, BUF_SIZE);
        prop_assert!(no_nan_or_inf(&buf));
    }

    #[test]
    fn sid_never_clips(note in 21u8..108, vel in 1u8..127) {
        let mut chip = Sid6581::new(SAMPLE_RATE);
        chip.voice_on(0, note, vel, 0.0);
        chip.voice_on(1, note.saturating_add(4).min(107), vel, 0.0);
        chip.voice_on(2, note.saturating_add(7).min(107), vel, 0.0);
        let buf = gen_samples(&mut chip, 4096); // longer for ADSR ramp
        prop_assert!(no_clipping(&buf));
    }

    #[test]
    fn sid_silent_after_reset(note in 21u8..108) {
        let mut chip = Sid6581::new(SAMPLE_RATE);
        chip.voice_on(0, note, 100, 0.0);
        gen_samples(&mut chip, 2048);
        chip.reset();
        let buf = gen_samples(&mut chip, 256);
        prop_assert!(buf.iter().all(|s| s.left == 0.0 && s.right == 0.0));
    }

    #[test]
    fn sid_waveform_all_valid(waveform in 0u8..4, note in 36u8..96) {
        let mut chip = Sid6581::new(SAMPLE_RATE);
        chip.set_param(0, waveform as f32); // PARAM_WAVEFORM
        chip.voice_on(0, note, 100, 0.0);
        let buf = gen_samples(&mut chip, 4096);
        prop_assert!(no_nan_or_inf(&buf));
    }

    #[test]
    fn sid_adsr_all_values_valid(a in 0u8..16, d in 0u8..16, s in 0u8..16, r in 0u8..16, note in 36u8..96) {
        let mut chip = Sid6581::new(SAMPLE_RATE);
        chip.set_param(2, a as f32);  // attack
        chip.set_param(3, d as f32);  // decay
        chip.set_param(4, s as f32);  // sustain
        chip.set_param(5, r as f32);  // release
        chip.voice_on(0, note, 100, 0.0);
        let buf = gen_samples(&mut chip, 4096);
        prop_assert!(no_nan_or_inf(&buf));
        prop_assert!(no_clipping(&buf));
    }
}
