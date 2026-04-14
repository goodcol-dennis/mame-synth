# ADR-003: Macro engine ticks at frame rate, not per-sample

## Context
Instrument macros (arpeggios, volume envelopes) modulate chip parameters over time. Two timing options: tick per audio sample (~44100 Hz) or tick per "frame" (~60 Hz, matching original hardware).

## Decision
Tick macros at frame rate (~60 Hz, every 735 samples at 44100 Hz) inside `ChipBank::generate_samples()`.

## Alternatives considered
- **Per-sample ticking**: Most granular, but 44100 ticks/sec of macro logic per voice is expensive and unnecessary — real hardware updated registers at VBlank (50/60 Hz).
- **Separate thread**: Avoids audio thread overhead, but adds synchronization complexity.
- **GUI-side ticking**: Would tick at repaint rate (~60fps), but couples macro timing to frame rate which varies with GPU load.

## Consequences
- **Easier**: Matches original hardware behavior. Cheap — 60 macro ticks per second vs 44100 per-sample ops. Lives in the audio thread so no sync issues.
- **Harder**: Arpeggio transitions are quantized to ~16.7ms steps. Very fast arpeggios (>60 per second) can't be represented — but real hardware had the same limitation.
- **Trade-off**: Authenticity over precision. The frame-rate approach sounds correct because it IS how the original hardware worked.
