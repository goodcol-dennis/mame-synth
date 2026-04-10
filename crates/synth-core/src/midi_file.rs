use midly::{MidiMessage, Smf, TrackEventKind};
use std::path::Path;

/// A single MIDI event with an absolute timestamp in microseconds.
#[derive(Debug, Clone, Copy)]
pub struct TimedMidiEvent {
    pub time_us: u64,
    pub note: u8,
    pub velocity: u8,
    pub is_on: bool,
}

/// Parsed MIDI file ready for playback.
pub struct MidiSequence {
    pub events: Vec<TimedMidiEvent>,
    pub duration_us: u64,
    pub name: String,
}

impl MidiSequence {
    /// Load and parse a standard MIDI file.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let data = std::fs::read(path)?;
        let smf = Smf::parse(&data)?;

        let ticks_per_beat = match smf.header.timing {
            midly::Timing::Metrical(tpb) => tpb.as_int() as u64,
            midly::Timing::Timecode(fps, sub) => {
                // Convert SMPTE to approximate ticks per beat
                (fps.as_int() as u64) * (sub as u64)
            }
        };

        let mut events = Vec::new();
        let mut tempo_us_per_beat: u64 = 500_000; // default 120 BPM

        // Merge all tracks into a single event list with absolute timestamps
        for track in &smf.tracks {
            let mut abs_tick: u64 = 0;
            let mut current_tempo = tempo_us_per_beat;

            for event in track {
                abs_tick += event.delta.as_int() as u64;
                let time_us = ticks_to_us(abs_tick, current_tempo, ticks_per_beat);

                match event.kind {
                    TrackEventKind::Meta(midly::MetaMessage::Tempo(t)) => {
                        current_tempo = t.as_int() as u64;
                        tempo_us_per_beat = current_tempo;
                    }
                    TrackEventKind::Midi { message, .. } => match message {
                        MidiMessage::NoteOn { key, vel } => {
                            let velocity = vel.as_int();
                            if velocity > 0 {
                                events.push(TimedMidiEvent {
                                    time_us,
                                    note: key.as_int(),
                                    velocity,
                                    is_on: true,
                                });
                            } else {
                                events.push(TimedMidiEvent {
                                    time_us,
                                    note: key.as_int(),
                                    velocity: 0,
                                    is_on: false,
                                });
                            }
                        }
                        MidiMessage::NoteOff { key, .. } => {
                            events.push(TimedMidiEvent {
                                time_us,
                                note: key.as_int(),
                                velocity: 0,
                                is_on: false,
                            });
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        // Sort by time
        events.sort_by_key(|e| e.time_us);
        let duration_us = events.last().map(|e| e.time_us).unwrap_or(0);

        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".into());

        Ok(MidiSequence {
            events,
            duration_us,
            name,
        })
    }
}

fn ticks_to_us(ticks: u64, tempo_us_per_beat: u64, ticks_per_beat: u64) -> u64 {
    if ticks_per_beat == 0 {
        return 0;
    }
    ticks * tempo_us_per_beat / ticks_per_beat
}

/// Plays a MidiSequence by feeding events at the right time.
pub struct MidiPlayer {
    sequence: Option<MidiSequence>,
    cursor: usize,
    start_time: Option<std::time::Instant>,
    paused_at: Option<u64>, // microseconds into the sequence
    playing: bool,
}

impl Default for MidiPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiPlayer {
    pub fn new() -> Self {
        MidiPlayer {
            sequence: None,
            cursor: 0,
            start_time: None,
            paused_at: None,
            playing: false,
        }
    }

    pub fn load(&mut self, seq: MidiSequence) {
        self.sequence = Some(seq);
        self.cursor = 0;
        self.start_time = None;
        self.paused_at = None;
        self.playing = false;
    }

    pub fn play(&mut self) {
        if self.sequence.is_none() {
            return;
        }
        if let Some(paused) = self.paused_at.take() {
            // Resume from paused position
            self.start_time =
                Some(std::time::Instant::now() - std::time::Duration::from_micros(paused));
        } else {
            self.start_time = Some(std::time::Instant::now());
            self.cursor = 0;
        }
        self.playing = true;
    }

    pub fn pause(&mut self) {
        if self.playing {
            self.paused_at = Some(self.elapsed_us());
            self.playing = false;
        }
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.cursor = 0;
        self.start_time = None;
        self.paused_at = None;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn has_sequence(&self) -> bool {
        self.sequence.is_some()
    }

    pub fn sequence_name(&self) -> &str {
        self.sequence
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or("")
    }

    pub fn duration_us(&self) -> u64 {
        self.sequence.as_ref().map(|s| s.duration_us).unwrap_or(0)
    }

    pub fn position_us(&self) -> u64 {
        if self.playing {
            self.elapsed_us()
        } else {
            self.paused_at.unwrap_or(0)
        }
    }

    pub fn progress(&self) -> f32 {
        let dur = self.duration_us();
        if dur == 0 {
            return 0.0;
        }
        (self.position_us() as f32 / dur as f32).clamp(0.0, 1.0)
    }

    /// Poll for events that should fire now. Returns note on/off events.
    pub fn poll(&mut self) -> Vec<TimedMidiEvent> {
        let mut fired = Vec::new();
        if !self.playing {
            return fired;
        }
        let seq = match &self.sequence {
            Some(s) => s,
            None => return fired,
        };

        let now_us = self.elapsed_us();

        while self.cursor < seq.events.len() {
            let event = &seq.events[self.cursor];
            if event.time_us <= now_us {
                fired.push(*event);
                self.cursor += 1;
            } else {
                break;
            }
        }

        // Check if we've reached the end
        if self.cursor >= seq.events.len() {
            self.playing = false;
        }

        fired
    }

    fn elapsed_us(&self) -> u64 {
        self.start_time
            .map(|t| t.elapsed().as_micros() as u64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_player_lifecycle() {
        let mut player = MidiPlayer::new();
        assert!(!player.is_playing());
        assert!(!player.has_sequence());

        let seq = MidiSequence {
            events: vec![
                TimedMidiEvent {
                    time_us: 0,
                    note: 60,
                    velocity: 100,
                    is_on: true,
                },
                TimedMidiEvent {
                    time_us: 500_000,
                    note: 60,
                    velocity: 0,
                    is_on: false,
                },
            ],
            duration_us: 500_000,
            name: "Test".into(),
        };

        player.load(seq);
        assert!(player.has_sequence());
        assert!(!player.is_playing());
        assert_eq!(player.sequence_name(), "Test");

        player.play();
        assert!(player.is_playing());

        // Poll immediately — should get the first event (time_us=0)
        let events = player.poll();
        assert!(!events.is_empty());
        assert!(events[0].is_on);

        // Small sleep to ensure elapsed time > 0
        std::thread::sleep(std::time::Duration::from_millis(1));

        player.pause();
        assert!(!player.is_playing());
        // Position should be >= 0 (may be 0 on very fast machines)
        // The key assertion is that pause captures a position
        let paused_pos = player.position_us();
        assert!(
            paused_pos < 500_000,
            "Should not have advanced past the sequence"
        );

        player.stop();
        assert!(!player.is_playing());
        assert_eq!(player.position_us(), 0);
    }
}
