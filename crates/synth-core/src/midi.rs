use midir::{MidiInput, MidiInputConnection};
use rtrb::Producer;

use crate::messages::AudioMessage;

#[derive(Default)]
pub struct MidiHandler {
    connection: Option<MidiInputConnection<()>>,
}

impl MidiHandler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Scan for available MIDI input ports. Returns port names.
    pub fn scan_ports() -> Vec<String> {
        let Ok(midi_in) = MidiInput::new("mame-synth-scan") else {
            return Vec::new();
        };
        midi_in
            .ports()
            .iter()
            .filter_map(|p| midi_in.port_name(p).ok())
            .collect()
    }

    /// Connect to a MIDI port by index.
    pub fn connect(&mut self, port_index: usize, producer: Producer<AudioMessage>) -> bool {
        // Need a fresh MidiInput for the connection (midir consumes it)
        let Ok(midi_in) = MidiInput::new("mame-synth") else {
            return false;
        };
        let ports = midi_in.ports();
        if port_index >= ports.len() {
            return false;
        }
        let port = &ports[port_index];

        match midi_in.connect(port, "mame-synth-input", midi_callback(producer), ()) {
            Ok(conn) => {
                self.connection = Some(conn);
                true
            }
            Err(e) => {
                log::error!("Failed to connect MIDI: {}", e);
                false
            }
        }
    }

    pub fn disconnect(&mut self) {
        self.connection = None;
    }

    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }
}

fn midi_callback(mut producer: Producer<AudioMessage>) -> impl FnMut(u64, &[u8], &mut ()) + Send {
    move |_timestamp, data, _| {
        if let Some(msg) = parse_midi(data) {
            let _ = producer.push(msg);
        }
    }
}

fn parse_midi(data: &[u8]) -> Option<AudioMessage> {
    if data.is_empty() {
        return None;
    }
    let status = data[0] & 0xF0;
    match status {
        // Note On (with velocity > 0)
        0x90 if data.len() >= 3 && data[2] > 0 => Some(AudioMessage::NoteOn {
            note: data[1],
            velocity: data[2],
        }),
        // Note Off, or Note On with velocity 0
        0x80 if data.len() >= 3 => Some(AudioMessage::NoteOff { note: data[1] }),
        0x90 if data.len() >= 3 => Some(AudioMessage::NoteOff { note: data[1] }),
        // Pitch Bend
        0xE0 if data.len() >= 3 => {
            let value = ((data[2] as u16) << 7) | data[1] as u16;
            Some(AudioMessage::PitchBend { value })
        }
        _ => None,
    }
}
