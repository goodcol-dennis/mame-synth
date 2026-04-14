# mame-synth

Rack-style synthesizer using classic video game sound chip emulators. Desktop app (Linux), Rust + egui.

## Process Audit Reference
- Development guardrails (core): https://raw.githubusercontent.com/goodcol-dennis/umami/refs/heads/main/umami.md

## Quick Reference

```bash
cargo build --release          # Build (includes YMFM C++ compilation)
cargo run --release             # Run (MUST use --release for audio performance)
cargo test                      # Run all tests
cargo fmt && cargo clippy       # Lint
```

## Architecture

```
GUI thread (egui/eframe)
    ↕ rtrb ring buffer (AudioMessage / GuiMessage)
Audio thread (cpal callback)
    → ChipBank → VoiceAllocator → SoundChip instances → stereo samples
```

- **synth-core** — audio engine, chip emulators, MIDI, voice allocation
- **synth-gui** — egui rack UI, custom widgets, chip panels
- **src/main.rs** — binary entry point, wires everything together

## Sound Chips

| Chip | Implementation | Voices | Notes |
|------|---------------|--------|-------|
| SN76489 (PSG) | Pure Rust | 3 tone + 1 noise | Sega Master System / Genesis |
| YM2612 (FM) | YMFM C++ via FFI | 6 channels | Sega Genesis, 4-operator FM |
| SID 6581 (C64) | Pure Rust | 3 voices | Triangle/saw/pulse/noise + ADSR |
| AY-3-8910 (PSG) | Pure Rust | 3 + noise | ZX Spectrum, MSX, Atari ST |
| Ricoh 2A03 (NES) | Pure Rust | 2 pulse + tri + noise | NES/Famicom APU |
| POKEY (Atari) | Pure Rust | 4 channels | Atari 800, polynomial distortion |
| YM2151 (OPM) | YMFM C++ via FFI | 8 channels | Arcade, Sharp X68000 |
| YM3812 (OPL2) | YMFM C++ via FFI | 9 channels | AdLib, Sound Blaster |
| YMF262 (OPL3) | YMFM C++ via FFI | 18 channels | Sound Blaster Pro 2 |
| SCC (Konami) | Pure Rust | 5 channels | MSX cartridges, wavetable |
| Namco WSG | Pure Rust | 3 channels | Pac-Man, wavetable |

## Key Abstractions

- **`SoundChip` trait** (`chip.rs`) — voice-level API: `voice_on(voice, note, velocity, detune_cents)`, `voice_off(voice)`, `generate_samples()`
- **`ChipBank`** (`voice.rs`) — pools N chip instances, total voices = N × chip.num_voices()
- **`VoiceAllocator`** (`voice.rs`) — Mono/Poly/Unison mode, handles note-to-voice mapping
- **`AudioMessage`** (`messages.rs`) — lock-free commands from GUI→audio thread (all `Copy`)
- **`InstrumentMacro`** (`macros.rs`) — arpeggio/volume/duty sequences at frame rate

## Adding a New Chip

See [CODEBASE.md](CODEBASE.md) for the full 7-step walkthrough. Summary:
1. Create chip `.rs` implementing `SoundChip` trait
2. Register in `chip.rs`, `audio.rs`, `patch.rs`, `lib.rs`
3. Tests auto-cover via `ChipId::all()` iteration in harness

## Known Issues

- **Wayland key repeat**: egui marks the initial keypress as `repeat: true` in some cases. Workaround: use raw `Event::Key` events, deduplicate per-key per-frame, take last state.
- **Debug builds unusable**: Chip emulation is too slow in debug mode — audio underruns. Always use `--release`.

## Acknowledged Gaps

| Gap | Severity | Status | Notes |
|-----|----------|--------|-------|
| No CI pipeline | Low | Deferred | Pre-commit hook enforces locally. CI can wait until contributors join. |
| rtrb SPSC limits MIDI input | Medium | Open | Can't connect hardware MIDI and GUI to same ring buffer. Need second channel or MPSC queue. |
| Macro volume modulation | Low | Open | Macro engine applies arpeggio but volume modulation needs per-voice volume API on chips. |

## YMFM FFI

The YM2612 uses [YMFM](https://github.com/aaronsgiles/ymfm) (BSD-3, git submodule at `crates/synth-core/ymfm/`). Flat C wrapper in `wrapper/ymfm_wrapper.cpp`, hand-written FFI in `ym2612_ffi.rs`. Build via `cc` crate in `build.rs`. Requires a C++17 compiler.

## Dependencies

System packages needed: `libasound2-dev` (ALSA headers for cpal).
