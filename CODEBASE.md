# Codebase Guide

Pre-derived understanding for AI assistants and contributors. Reduces re-exploration cost.

## Module Map

```
crates/synth-core/src/
├── chip.rs          ChipId enum (11 chips), SoundChip trait, VoiceMode, ParamInfo
├── audio.rs         cpal audio engine, progressive buffer selection, create_bank()
├── voice.rs         VoiceAllocator (Poly/Mono/Unison) + ChipBank + macro integration
├── macros.rs        InstrumentMacro engine (arpeggio, volume, duty sequences)
├── messages.rs      AudioMessage / GuiMessage enums (all Copy, for rtrb)
├── patch.rs         Patch JSON serialization, PatchBank, 14 factory presets
├── midi.rs          Hardware MIDI input via midir
├── midi_file.rs     MIDI file parser (midly), MidiPlayer with transport
├── vgm.rs           VGM file parser (all chip types, VGZ gzip support)
├── vgm_extract.rs   VGM → patches + MIDI extraction (per-chip analyzers)
├── sid_extract.rs   SID file extraction via embedded 6502 CPU emulator
│
│  ── Sound Chips (Pure Rust) ──
├── sn76489.rs       SN76489 PSG (3 square + 1 noise)
├── sid6581.rs       SID 6581 (3 voices + ADSR + state-variable filter)
├── ay8910.rs        AY-3-8910 PSG (3 square + noise + poly counters)
├── ricoh2a03.rs     Ricoh 2A03 NES APU (2 pulse + tri + noise)
├── pokey.rs         POKEY (4 channels + polynomial distortion)
├── scc.rs           SCC Konami wavetable (5 channels, 32-byte waveforms)
├── namco_wsg.rs     Namco WSG Pac-Man wavetable (3 channels)
│
│  ── Sound Chips (YMFM C++ FFI) ──
├── ym2612.rs        YM2612 FM (6-channel 4-op, Genesis)
├── ym2151.rs        YM2151 OPM (8-channel 4-op, arcade)
├── ym3812.rs        YM3812 OPL2 (9-channel 2-op, AdLib)
├── ymf262.rs        YMF262 OPL3 (18-channel 2-op stereo)
├── ym2612_ffi.rs    Raw extern "C" bindings for YM2612
├── ym2151_ffi.rs    Raw extern "C" bindings for YM2151
├── opl_ffi.rs       Raw extern "C" bindings for OPL2 + OPL3
├── wrapper/         C++ wrappers (ymfm_wrapper, ymfm_opm_wrapper, ymfm_opl_wrapper)
└── ymfm/            Git submodule — YMFM source (BSD-3)

crates/synth-gui/src/
├── app.rs           MameSynthApp struct, state, update loop, chip/patch/mode selectors
├── input.rs         Computer keyboard handling, file-polling test command protocol
├── transport.rs     MIDI transport bar (import VGM/SID, play/pause/stop, progress)
├── rack_panel.rs    Generic param→knob/toggle renderer from ParamInfo metadata
├── theme.rs         Dark rack-style color constants
├── panels/          Chip-specific header panels (sn76489, ym2612, sid6581)
└── widgets/         Custom egui widgets (knob, keyboard, vu_meter, waveform)

tests/
└── e2e_wayland.rs   Headless Wayland E2E (cage + file-polling, 9 tests)

docs/decisions/      Architecture Decision Records (ADRs)
```

## How to Add a New Sound Chip

1. **Create `crates/synth-core/src/<chip>.rs`**
   - Implement `SoundChip` trait: `chip_id()`, `num_voices()`, `param_info()`, `set_param()`, `get_param()`, `voice_on()`, `voice_off()`, `generate_samples()`, `reset()`
   - Pure Rust for simple chips, C++ FFI wrapper for YMFM chips
   - Include `#[cfg(test)] mod tests` with at least: silent after creation, produces sound, reset silences

2. **Register in `chip.rs`**: Add variant to `ChipId`, `all()`, `display_name()`, `param_info_for_chip()`

3. **Register in `audio.rs`**: Add import + match arm in `create_bank()`

4. **Register in `patch.rs`**: Add to `chip_id_to_str()` and `str_to_chip_id()`

5. **Register in `lib.rs`**: Add `pub mod <chip>;` (and `mod <chip>_ffi;` if FFI)

6. **Add GUI panel (optional)**: New chips use the default `_` arm in app.rs which shows `display_name()`. Custom panels only needed for complex chips like FM algorithm diagrams.

7. **Tests auto-cover**: `no_nan_through_pipeline_all_chips` and `param_changes_during_playback_stable` in `audio_harness.rs` iterate `ChipId::all()`.

## Agent Delegation Guide

When to use each model:

| Model | Use for | Example |
|-------|---------|---------|
| **Haiku** | Search, grep, file reads, simple questions | "Find all uses of ChipId" |
| **Sonnet** | Implementing chips following the pattern, writing tests, clippy/fmt fixes, UI wiring, doc updates | "Add Game Boy APU following sn76489.rs pattern" |
| **Opus** | Architecture decisions, debugging audio issues, cross-module refactors, umami audits, extraction pipeline design | "Design the macro-to-chip integration" |

**Briefing a Sonnet agent for a new chip:**
- Reference an existing chip file as the template (e.g., "follow sn76489.rs")
- List the 7 registration steps from this doc
- Specify chip parameters and their ranges
- Tell it to run `cargo fmt --all` and list which files NOT to touch

**Briefing a Sonnet agent for UI changes:**
- Reference the existing pattern (e.g., "follow how chip_count control works")
- Specify the `AudioMessage` variant if one is needed
- Tell it to only edit the GUI crate files

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
                    │  Macro selector → SetMacro│
                    │  Chip count → SetChipCount│
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
                    │  Macro engine ticks @60Hz │
                    │    → arpeggio retrigger   │
                    │  ChipBank.generate()      │
                    │  → interleave + waveform  │
                    └──────────┬───────────────┘
                               │ rtrb::Producer<GuiMessage>
                               ▼
                    ┌──────────────────────────┐
                    │  GUI: VU meters, waveform │
                    └──────────────────────────┘
```

## Testing Patterns

- **Unit tests**: Inline `#[cfg(test)]` per chip module. Test pure logic.
- **Property tests**: `tests/chip_properties.rs` (proptest). No NaN, no clipping, silence after reset.
- **Integration harness**: `tests/audio_harness.rs`. Full pipeline without cpal.
- **E2E tests**: `tests/e2e_wayland.rs`. Headless cage + file-polling protocol.

```bash
# Fast (unit + property + harness):
cargo test --release --workspace --lib --test chip_properties --test audio_harness

# E2E (needs cage):
cargo test --release --test e2e_wayland -- --nocapture --test-threads=1

# All:
cargo test --release
```

## Non-Obvious Constraints

- **Always `--release`**: Debug mode too slow for real-time audio.
- **Progressive buffer selection**: Tries 128→256→512→1024, picks smallest working. See ADR-004.
- **Wayland key repeat**: egui marks initial presses as `repeat: true`. Use raw Event::Key with dedup.
- **rtrb is SPSC**: One producer per ring buffer. Hardware MIDI needs a second channel.
- **Macros tick at frame rate**: ~60Hz in audio callback. Matches original hardware. See ADR-003.
- **E2E uses file-polling**: Tests write to `/tmp/mame-synth-input.txt`, not key injection. See ADR-002.
