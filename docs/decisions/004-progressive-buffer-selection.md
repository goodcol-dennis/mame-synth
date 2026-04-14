# ADR-004: Progressive buffer size selection for audio latency

## Context
Audio latency depends on the cpal buffer size. `BufferSize::Default` gives the OS-chosen buffer (~4410 frames = ~100ms on our PipeWire system). `BufferSize::Fixed(N)` requests a specific size but silently kills the audio callback on some backends.

## Decision
Try buffer sizes 128→256→512→1024 with a dummy stream probe. Use the smallest size that `build_output_stream` accepts. Fall back to `Default` if none work.

## Alternatives considered
- **Always Default**: Safe but ~100ms latency — unplayable for a synth.
- **Always Fixed(256)**: Worked in testing but previously crashed on some systems. No way to know in advance.
- **PIPEWIRE_QUANTUM env var**: Per-process quantum hint. Didn't work — cpal's ALSA backend doesn't pass it through.
- **JACK backend**: Low latency guaranteed, but requires JACK server running. Not universal.

## Consequences
- **Easier**: Self-adapting to the hardware. On our system: 128 frames = 2.9ms. On systems that can't do 128, falls back gracefully.
- **Harder**: The dummy stream probe adds ~50ms to startup. The probe passing doesn't guarantee the real callback won't crash (though in practice it does).
- **Result**: 2.9ms latency on PipeWire/Intel. 34x improvement over Default.
