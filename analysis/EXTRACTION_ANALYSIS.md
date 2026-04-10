# VGM Extraction Analysis

## What We Built

A VGM parser (`crates/synth-core/src/vgm.rs`) that:
- Parses VGM v1.00–v1.72 headers
- Decodes command streams for all 7 supported chips
- Tracks absolute sample positions (at 44100 Hz)
- Handles VGZ (gzip-compressed) files
- Generates synthetic test VGMs for validation

Validated with a synthetic SN76489 VGM: C major scale, 8 notes, 32 register writes, 2.4s duration.

## Extraction Pipeline: VGM → Patches + MIDI

### Step 1: Parse VGM → Register Writes with Timing

This is done. The parser outputs `Vec<TimedCommand>` with sample-accurate positions.

### Step 2: Detect Key-On Events (Chip-Specific)

Each chip signals "a note is starting" differently:

| Chip | Key-On Detection | Frequency Register | Notes |
|------|-----------------|-------------------|-------|
| **SN76489** | Volume register changes from 0xF (silent) to non-0xF | Tone period (10-bit) in latch/data pair | No explicit key-on; volume = the gate |
| **YM2612** | Register 0x28 write with operator bits set | Frequency in 0xA0-0xA6 (F-num + block) | Key-on is explicit, per-channel |
| **YM2151** | Register 0x08 write with slot bits set | KC (0x28-0x2F) + KF (0x30-0x37) | Key-on is explicit, per-channel |
| **AY-3-8910** | Volume register (R8-RA) changes from 0 to non-0 | Tone period in R0-R5 (12-bit, split across 2 regs) | Like SN76489, volume = gate |
| **2A03 (NES)** | Length counter load (reg 0x4003/07/0B/0F bit 3-7) | Period in 0x4002-03, 0x4006-07, 0x400A-0B | Somewhat implicit; length counter triggers restart |
| **POKEY** | AUDCTL/AUDC changes + volume | AUDF registers | No explicit key-on; continuous |
| **SID** | Gate bit in control register (0xD404, 0xD40B, 0xD412) | Frequency in 0xD400-01, 0xD407-08, 0xD40E-0F | Explicit gate on/off bit |

### Step 3: Snapshot Register State at Key-On → Patch

When we detect a key-on event, we snapshot ALL registers that define the "sound" of that channel. This becomes a patch.

**What goes in a patch (per chip):**

| Chip | Patch Registers | Count |
|------|----------------|-------|
| SN76489 | Just the volume level (only 4 bits of "sound shaping") | 1 |
| YM2612 | Algorithm, feedback, 4× operator (TL, AR, D1R, D2R, SL, RR, MUL, DT) = 34 values | 34 |
| YM2151 | Same as YM2612 but with DT2 and key scaling | ~36 |
| SID | Waveform, pulse width, ADSR, filter settings | ~10 |
| AY-3-8910 | Volume, mixer bits, noise period, envelope shape/period | ~6 |
| 2A03 | Duty cycle, length counter, sweep, envelope | ~4 per channel type |
| POKEY | AUDC (distortion), AUDCTL (clock mode) | ~3 |

### Step 4: Convert Frequency → MIDI Note Number

Each chip stores frequency differently. Conversion:

| Chip | Frequency → Hz | Hz → MIDI |
|------|---------------|-----------|
| SN76489 | `Hz = clock / (32 * N)` where N = 10-bit period | `note = 69 + 12 * log2(Hz / 440)` |
| YM2612 | `Hz = (fnum * 2^block * clock) / (144 * 2^21)` | Same formula |
| YM2151 | KC/KF → Hz via lookup table | Direct from KC |
| SID | `Hz = (freq_reg * clock) / 16777216` | Same formula |
| AY-3-8910 | `Hz = clock / (16 * N)` where N = 12-bit period | Same formula |
| 2A03 | `Hz = clock / (16 * (period + 1))` for pulse | Same formula |
| POKEY | `Hz = clock / (28 * 2 * (divider + 1))` | Same formula |

### Step 5: Output

- **Patches**: JSON files matching our existing `Patch` format
- **MIDI sequences**: Standard MIDI files (.mid) or our `MidiSequence` objects
- **Metadata**: Source game, composer, chip, original filename

## Gaps Identified

### Gap 1: Our SN76489 Has No Real "Patch"
The SN76489 is just a square wave with volume. There's nothing to extract except volume level. Real SN76489 music uses **rapid register writes** (arpeggios, vibrato, volume envelopes) that are part of the music engine, not the chip. Our synth only has static knobs — we're missing the concept of a "software instrument" (macro/sequence of register writes over time).

**Recommendation**: Add a "macro" or "instrument sequence" layer that can replay timed register writes. This is how trackers (Deflemask, Furnace) work. Without it, SN76489/AY-3-8910/POKEY patches will sound static and lifeless compared to the originals.

### Gap 2: No Filter Emulation on SID
The real SID has a resonant multimode filter (LP/BP/HP). Many iconic SID sounds depend on filter sweeps. Our SID emulation has no filter. Patches extracted from Rob Hubbard tunes will sound wrong.

**Recommendation**: Add SID filter emulation (12dB/oct state-variable filter). This is the single biggest improvement for SID authenticity.

### Gap 3: YM2612 Operator Register Layout Mismatch
VGM files write registers using the real chip's layout where operators are at offsets +0, +4, +8, +12 within a channel. Our `set_param` uses a logical ID scheme (100+op*100+offset). The extraction tool needs a mapping layer between VGM register addresses and our param IDs.

**Recommendation**: Add `register_to_param_id()` and `param_id_to_register()` conversion functions per chip. This also enables future "register view" in the GUI.

### Gap 4: No Multi-Track/Polyphonic Extraction
A VGM file uses multiple channels simultaneously. Our extraction needs to decide: extract as a single polyphonic MIDI file (one track per channel) or as separate patches per channel.

**Recommendation**: Extract both — a multi-track MIDI file for playback AND individual patches per unique instrument configuration seen.

### Gap 5: Timing Resolution
VGM timing is at 44100 Hz (sample-accurate). MIDI timing is in ticks/beat. The conversion needs careful tempo mapping — most game music runs at 50/60 Hz frame rate, which maps to specific BPM values (e.g., 60 Hz with 6 ticks per note = 150 BPM).

**Recommendation**: Default to the VGM's rate field (50 or 60 Hz) and compute BPM from the average note duration found in the VGM.

### Gap 6: VGM Files Not Easily Downloadable
VGMRips requires browsing their website — no direct download API. Users need to manually download VGM packs.

**Recommendation**: The tool should accept a directory of VGM files and batch-process them. Include clear instructions pointing users to vgmrips.net, SMS Power, HVSC, and Zophar's Domain. Don't bundle copyrighted VGMs.

### Gap 7: SID and NSF Use Different Formats
SID files (.sid) and NES files (.nsf) are NOT VGM — they contain executable 6502 code that must be emulated to produce register writes. We'd need a SID/NSF player that logs register writes, then feed those logs into our extractor.

**Recommendation**: Phase 1 focuses on VGM files (covers SN76489, YM2612, YM2151, AY-3-8910, NES APU, POKEY). Phase 2 adds .sid support via a 6502 emulator or ChiptuneSAK integration. The tool should be modular enough to accept register write logs from any source.

## Tool Design: `mame-synth-extract`

A CLI tool that lives in this project:

```
mame-synth-extract <input.vgm> [--output-dir patches/] [--midi output.mid]
```

Features:
- Parse VGM/VGZ files
- Detect chips used
- For each channel: extract unique patches (register snapshots at key-on)
- Convert note events to MIDI
- Output: JSON patches compatible with our patch system + standard MIDI files
- Batch mode: process a directory of VGMs

Architecture:
```
vgm.rs (parser)         — we have this
  → chip_analyzer.rs    — per-chip register interpretation
    → patch_extractor   — snapshot registers at key-on → Patch JSON
    → midi_extractor    — frequency + timing → MIDI events
  → extract CLI binary  — wires it together
```

## Recommended Demo VGMs (by priority)

### Must-Have (Phase 1 — prove the pipeline works)
1. **SN76489**: Any SMS game VGM from smspower.org
2. **YM2612**: Streets of Rage 2 or Sonic 2 from vgmrips.net
3. **YM2151**: Street Fighter 2 arcade from vgmrips.net

### Nice-to-Have (Phase 2 — expand coverage)
4. **AY-3-8910**: ZX Spectrum game from zxart.ee
5. **NES (VGM variant)**: Mega Man 2 from vgmrips.net
6. **POKEY**: Atari game (limited VGM availability — may need SAP format support)
7. **SID**: Requires .sid format support (HVSC) — Phase 2

## Next Steps

1. Build `chip_analyzer.rs` with per-chip key-on detection and register snapshots
2. Build `midi_extractor.rs` for frequency-to-MIDI conversion
3. Create the `mame-synth-extract` CLI binary
4. Test with synthetic VGMs first, then real VGMs from archives
5. Document the extraction process for users
