# mame-synth Roadmap

## Current State (v0.1)

- 3 sound chips: SN76489 (PSG), YM2612 (FM), SID 6581 (C64)
- Rack-style egui GUI with knob widgets
- Computer keyboard input (Z-M keys)
- Voice modes: Poly, Mono, Unison
- VoiceAllocator + ChipBank architecture
- 42 tests (unit + property-based + integration harness)

## Planned Features

### Patches (Save/Load)
- Serialize chip params + voice mode to JSON
- Patch bank per chip type in `patches/` directory
- GUI: dropdown to select/save/rename patches
- Factory presets for each chip (classic sounds)

### MIDI File Playback
- Parse standard MIDI files (.mid) using `midly` crate
- Transport controls: play / pause / stop / seek
- Tempo display and BPM control
- Route MIDI channels to chip voices
- Visual playback position indicator

### Additional Sound Chips (Priority Order)
*See chip priority list below*

### Hardware MIDI Input
- Connect real MIDI keyboards via `midir`
- MIDI CC mapping to chip parameters
- MIDI learn mode (click knob, move CC, mapped)
- Needs second rtrb channel (SPSC constraint)

### Audio Improvements
- Fix audio latency (investigate PipeWire native, JACK backend)
- Fix YM2612 idle output (init patch produces ~0.06 background level)
- Add master volume / limiter to prevent clipping
- Waveform oscilloscope display

### GUI Improvements
- Fix virtual keyboard mouse interaction (click/drag)
- Chip instance count control (add more chips for more voices)
- Unison detune spread visualization
- FM algorithm topology diagram (interactive for YM2612)
- Resizable window / responsive layout

### FPGA Integration (Future)
- Design register-write protocol over USB/SPI
- Support MiSTer DE10-Nano as alternative backend
- Same GUI controls hardware chips instead of software emulation
- Requires FPGA hardware (not currently available)

## Sound Chip Priority List

Chips ranked by popularity, musical interest, and implementation complexity.

### Tier 1 — High Priority (iconic, widely used)

| Chip | System | Type | Voices | Implementation |
|------|--------|------|--------|---------------|
| **AY-3-8910** | ZX Spectrum, MSX, Atari ST | PSG | 3 square + noise | Pure Rust (simple, like SN76489) |
| **Ricoh 2A03** | NES/Famicom | PSG + DPCM | 2 pulse + 1 tri + 1 noise + 1 DPCM | Pure Rust |
| **POKEY** | Atari 800/5200 | PSG | 4 channels | Pure Rust |
| **YM2151 (OPM)** | Arcade, Sharp X68000 | FM | 8 channels, 4-op | YMFM (already in submodule) |

### Tier 2 — Medium Priority (interesting, less common)

| Chip | System | Type | Voices | Implementation |
|------|--------|------|--------|---------------|
| **YM3812 (OPL2)** | AdLib, Sound Blaster | FM | 9 channels, 2-op | YMFM |
| **YMF262 (OPL3)** | Sound Blaster Pro 2 | FM | 18 channels, 4-op | YMFM |
| **SCC** | Konami MSX cartridges | Wavetable | 5 channels | Pure Rust |
| **Namco WSG** | Pac-Man, Galaga | Wavetable | 3-8 channels | Pure Rust |
| **SN76477** | Space Invaders | Analog | Complex | Pure Rust (analog model) |

### Tier 3 — Lower Priority (niche, complex)

| Chip | System | Type | Voices | Implementation |
|------|--------|------|--------|---------------|
| **SPC700** | SNES | DSP + BRR | 8 channels | Complex (BRR sample playback) |
| **Paula** | Amiga | PCM | 4 channels | Pure Rust (sample-based) |
| **RF5C68** | Sega CD | PCM | 8 channels | Pure Rust |
| **YM2610 (OPNB)** | Neo Geo | FM + ADPCM | 4 FM + 6 ADPCM | YMFM + ADPCM |
| **HuC6280** | TurboGrafx-16 | Wavetable | 6 channels | Pure Rust |
| **Game Boy APU** | Game Boy | PSG | 2 pulse + 1 wave + 1 noise | Pure Rust |

### Notes on Implementation Strategy

- **YMFM chips** (YM2151, OPL2, OPL3, YM2610): Already have the YMFM submodule. Each needs a thin C wrapper (like the YM2612 one) — ~50 lines of C++ per chip.
- **Pure Rust PSGs** (AY-3-8910, 2A03, POKEY, Game Boy): Similar complexity to SN76489. Register-based, straightforward state machines.
- **Wavetable chips** (SCC, Namco WSG, HuC6280): Need a small RAM buffer for waveform data. Medium complexity.
- **Sample-based chips** (SPC700, Paula): Require sample ROM/RAM infrastructure. Higher complexity.
