# ADR-001: YMFM over Nuked-OPN2 for FM chip emulation

## Context
We needed FM synthesis emulation for YM2612 (Genesis), YM2151 (arcade), and OPL2/OPL3 (PC). Two main options: YMFM (BSD-3, by a MAME developer) and Nuked-OPN2/Nuked-OPL3 (LGPL, cycle-accurate from die shots).

## Decision
Use YMFM for all Yamaha FM chips.

## Alternatives considered
- **Nuked-OPN2/OPL3**: More accurate (cycle-level), but LGPL license, higher CPU cost (~6x more cycles per sample), and each chip needs separate integration.
- **Pure Rust FM**: Would avoid C++ FFI entirely, but FM synthesis is complex — reimplementing 4-operator FM correctly would take weeks and still be less accurate.

## Consequences
- **Easier**: One YMFM submodule covers YM2612, YM2151, OPL2, OPL3, and future chips (YM2203, YM2610). Each new chip is ~50 lines of C wrapper.
- **Harder**: Requires C++17 compiler. FFI boundary means `unsafe` code. Can't inspect YMFM internals for debugging.
- **Trade-off**: Slightly less accurate than Nuked but more than sufficient for a playable synth. The accuracy difference is only audible in specific edge cases (SSG-EG, DAC crossover distortion).
