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

## Key Abstractions

- **`SoundChip` trait** (`chip.rs`) — voice-level API: `voice_on(voice, note, velocity, detune_cents)`, `voice_off(voice)`, `generate_samples()`
- **`ChipBank`** (`voice.rs`) — pools N chip instances, total voices = N × chip.num_voices()
- **`VoiceAllocator`** (`voice.rs`) — Mono/Poly/Unison mode, handles note-to-voice mapping
- **`AudioMessage`** (`messages.rs`) — lock-free commands from GUI→audio thread (all `Copy`)

## Adding a New Chip

See [CODEBASE.md](CODEBASE.md) for the full 7-step walkthrough. Summary:
1. Create chip `.rs` implementing `SoundChip` trait
2. Register in `chip.rs`, `audio.rs`, `patch.rs`, `lib.rs`
3. Tests auto-cover via `ChipId::all()` iteration in harness

## Known Issues

- **Audio latency ~100ms**: `BufferSize::Default` gives ~4410 frames on this system. `Fixed(256)` silently kills the audio callback. Root cause: cpal ALSA backend + PipeWire compatibility. Workaround: none yet — needs investigation into PipeWire native backend or JACK.
- **Wayland key repeat**: egui marks the initial keypress as `repeat: true` in some cases. Workaround: use raw `Event::Key` events, deduplicate per-key per-frame, take last state.
- **Debug builds unusable**: Chip emulation is too slow in debug mode — audio underruns. Always use `--release`.

## Acknowledged Gaps

| Gap | Severity | Status | Notes |
|-----|----------|--------|-------|
| Audio latency ~100ms | Medium | Open | cpal `Default` buffer is large. `Fixed(256)` crashes callback. Need PipeWire native or JACK investigation. |
| YM2612 idle output (~0.06 peak) | Low | Open | `init_default_patch()` leaves operators producing background noise. Need key-off on all channels at init. |
| E2E tests not passing in cage | Medium | Open | F11/F12 key injection timing issues with egui inside headless cage compositor. Tests scaffolded but need tuning. |
| No CI pipeline | Low | Deferred | Pre-commit hook enforces locally. CI can wait until contributors join. |
| rtrb SPSC limits MIDI input | Medium | Open | Can't connect hardware MIDI and GUI to same ring buffer. Need second channel or MPSC queue. |
| No multi-chip stacking in GUI | Low | Planned | ChipBank supports it. GUI needs chip count spinner control. |
| app.rs still 516 lines | Low | Acceptable | Down from 774. Remaining code is egui layout, hard to split further without readability loss. |

## YMFM FFI

The YM2612 uses [YMFM](https://github.com/aaronsgiles/ymfm) (BSD-3, git submodule at `crates/synth-core/ymfm/`). Flat C wrapper in `wrapper/ymfm_wrapper.cpp`, hand-written FFI in `ym2612_ffi.rs`. Build via `cc` crate in `build.rs`. Requires a C++17 compiler.

## Dependencies

System packages needed: `libasound2-dev` (ALSA headers for cpal).
