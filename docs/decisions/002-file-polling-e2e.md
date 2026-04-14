# ADR-002: File-polling over wtype for e2e test protocol

## Context
E2e tests run mame-synth inside a headless cage compositor. We need to send commands (switch chip, play notes) and read state (dump params, peak levels) from the test harness.

Initial approach: use `wtype -k F11/F12` to inject keypresses into the app, triggering test command handlers. This failed because egui inside cage doesn't reliably receive F-key events — the keys are swallowed or arrive as repeat events.

## Decision
File-polling protocol: tests write commands to `/tmp/mame-synth-input.txt`, the app polls this file every frame and deletes it after consumption. State dumps write to `/tmp/mame-synth-state.txt`.

## Alternatives considered
- **wtype key injection**: Simple but unreliable in headless cage. Worked 60% of the time.
- **TCP socket**: More robust than files, but adds networking complexity and port allocation.
- **D-Bus**: Standard Linux IPC, but heavy dependency for test commands.
- **Nested compositor (visible)**: Using parent WAYLAND_DISPLAY makes tests non-headless — user can accidentally interact.

## Consequences
- **Easier**: 100% reliable, no timing issues, works headlessly.
- **Harder**: Polling every frame costs a syscall. File I/O in the GUI thread isn't ideal. Race conditions if tests write faster than the app consumes (mitigated by waiting for file deletion).
- **Trade-off**: Keyboard input tests still need wtype for actual key injection verification, but the core command protocol doesn't depend on it.
