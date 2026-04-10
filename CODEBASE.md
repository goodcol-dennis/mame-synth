# Codebase Guide

Pre-derived understanding for AI assistants and contributors. Reduces re-exploration cost.

## Module Map

```
crates/synth-core/src/
├── chip.rs          ChipId enum, SoundChip trait, VoiceMode, ParamInfo types
├── audio.rs         cpal audio engine, create_bank(), audio_callback()
├── voice.rs         VoiceAllocator (Poly/Mono/Unison) + ChipBank (multi-chip pooling)
├── messages.rs      AudioMessage / GuiMessage enums (all Copy, for rtrb)
├── patch.rs         Patch JSON serialization, PatchBank, factory presets
├── midi.rs          Hardware MIDI input via midir
├── midi_file.rs     MIDI file parser (midly), MidiPlayer with transport
├── sn76489.rs       SN76489 PSG (pure Rust, 3 square + 1 noise)
├── sid6581.rs       SID 6581 (pure Rust, 3 voices + ADSR)
├── ay8910.rs        AY-3-8910 PSG (pure Rust, 3 square + noise + poly)
├── ricoh2a03.rs     Ricoh 2A03 NES APU (pure Rust, 2 pulse + tri + noise)
├── pokey.rs         POKEY (pure Rust, 4 channels + polynomial distortion)
├── ym2612.rs        YM2612 FM (YMFM C++ via FFI, 6-channel 4-op)
├── ym2151.rs        YM2151 OPM (YMFM C++ via FFI, 8-channel 4-op)
├── ym2612_ffi.rs    Raw extern "C" bindings for YM2612
├── ym2151_ffi.rs    Raw extern "C" bindings for YM2151
├── wrapper/         C++ wrappers for YMFM (ymfm_wrapper.cpp, ymfm_opm_wrapper.cpp)
└── ymfm/            Git submodule — YMFM source (BSD-3)

crates/synth-gui/src/
├── app.rs           MameSynthApp struct, state, update loop, egui layout
├── input.rs         Computer keyboard handling, F11/F12 test command protocol
├── rack_panel.rs    Generic param→knob/toggle renderer from ParamInfo metadata
├── theme.rs         Dark rack-style color constants
├── panels/          Chip-specific header panels (sn76489, ym2612, sid6581)
└── widgets/         Custom egui widgets (knob, keyboard, vu_meter)

tests/
└── e2e_wayland.rs   Headless Wayland E2E tests (cage + wtype + wlrctl)
```

## How to Add a New Sound Chip

1. **Create `crates/synth-core/src/<chip>.rs`**
   - Implement `SoundChip` trait: `chip_id()`, `num_voices()`, `param_info()`, `set_param()`, `get_param()`, `voice_on()`, `voice_off()`, `generate_samples()`, `reset()`
   - Pure Rust for simple chips, C++ FFI wrapper for YMFM chips
   - Include `#[cfg(test)] mod tests` with at least: silent after creation, produces sound, reset silences

2. **Register in `chip.rs`**
   - Add variant to `ChipId` enum
   - Add to `ChipId::all()` and `display_name()`
   - Add to `param_info_for_chip()`

3. **Register in `audio.rs`**
   - Add import
   - Add match arm in `create_bank()`

4. **Register in `patch.rs`**
   - Add to `chip_id_to_str()` and `str_to_chip_id()`

5. **Register in `lib.rs`**
   - Add `pub mod <chip>;` (and `mod <chip>_ffi;` if FFI)

6. **Add GUI panel (optional)**
   - Create `crates/synth-gui/src/panels/<chip>_panel.rs`
   - Add match arm in `app.rs` chip header section
   - Generic chips use the default `_` arm which shows `display_name()`

7. **Tests auto-cover new chips**: `no_nan_through_pipeline_all_chips` and `param_changes_during_playback_stable` in `audio_harness.rs` iterate `ChipId::all()`.

## Message Flow

```
                    ┌──────────────────────────┐
                    │      GUI Thread           │
                    │  (egui update loop)       │
                    │                           │
                    │  Keyboard/Mouse → NoteOn  │
                    │  Knobs → SetParam         │
                    │  Chip selector → Switch   │
                    │  MIDI player → NoteOn/Off │
                    └──────────┬───────────────┘
                               │ rtrb::Producer<AudioMessage>
                               ▼
                    ┌──────────────────────────┐
                    │     Audio Thread          │
                    │  (cpal callback)          │
                    │                           │
                    │  Drain messages           │
                    │  → ChipBank.note_on()     │
                    │    → VoiceAllocator       │
                    │    → chip.voice_on()      │
                    │  ChipBank.generate()      │
                    │  → interleave to output   │
                    └──────────┬───────────────┘
                               │ rtrb::Producer<GuiMessage>
                               ▼
                    ┌──────────────────────────┐
                    │  GUI: VU meters, peaks    │
                    └──────────────────────────┘
```

## Testing Patterns

- **Unit tests**: Inline `#[cfg(test)]` in each chip module. Test pure logic without audio hardware.
- **Property tests**: `tests/chip_properties.rs` using `proptest`. Invariants: no NaN, no clipping, silence after reset.
- **Integration harness**: `tests/audio_harness.rs`. Drives full ChipBank + message pipeline without cpal. Tests chip switching, voice modes, parameter changes.
- **E2E tests**: `tests/e2e_wayland.rs`. Headless cage compositor + wtype/wlrctl. Tests full app via F11/F12 protocol.

Run tests:
```bash
# Unit + property + harness (fast, no GUI):
cargo test --release --workspace --lib --test chip_properties --test audio_harness

# E2E (slow, needs cage/wtype/wlrctl):
cargo test --release --test e2e_wayland -- --nocapture --test-threads=1
```

## Non-Obvious Constraints

- **Always build/run with `--release`**: Debug mode is too slow for real-time audio. Chips underrun.
- **`BufferSize::Fixed` crashes on some devices**: cpal accepts the config but the callback silently dies. Use `Default`.
- **Wayland key repeat**: egui marks initial keypresses as `repeat: true` in some cases. Input handler uses raw `Event::Key` with per-key-per-frame deduplication.
- **rtrb is SPSC**: One producer, one consumer per ring buffer. MIDI hardware input would need a second channel or a different queue.
- **YMFM chips have idle output**: YM2612 produces ~0.06 peak when no notes are playing due to initialized operator state.
