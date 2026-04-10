//! Generate demo VGM and MIDI files for testing.
//!
//! Run: cargo run --release --bin generate-demos

use std::path::Path;

fn main() {
    let demo_dir = Path::new("demos");
    std::fs::create_dir_all(demo_dir).unwrap();

    // 1. SN76489 VGM — C major scale
    let vgm_data = synth_core::vgm::create_test_vgm_sn76489();
    let vgm_path = demo_dir.join("sn76489_scale.vgm");
    std::fs::write(&vgm_path, &vgm_data).unwrap();
    println!("Created: {}", vgm_path.display());

    // 2. Simple MIDI file — melody
    let midi_path = demo_dir.join("demo_melody.mid");
    write_demo_midi(&midi_path);
    println!("Created: {}", midi_path.display());

    // 3. Chord progression MIDI
    let chord_path = demo_dir.join("demo_chords.mid");
    write_chord_midi(&chord_path);
    println!("Created: {}", chord_path.display());

    println!("\nDemo files in: {}/", demo_dir.display());
    println!("Open the app and use 'Import VGM' or 'Open MIDI' to load them.");
}

fn write_demo_midi(path: &Path) {
    // Standard MIDI file format 0 (single track)
    // Header: MThd, length=6, format=0, tracks=1, division=480 ticks/beat
    let mut data: Vec<u8> = Vec::new();

    // Header chunk
    data.extend_from_slice(b"MThd");
    data.extend_from_slice(&6u32.to_be_bytes());
    data.extend_from_slice(&0u16.to_be_bytes()); // format 0
    data.extend_from_slice(&1u16.to_be_bytes()); // 1 track
    data.extend_from_slice(&480u16.to_be_bytes()); // 480 ticks/beat

    // Track chunk
    let mut track: Vec<u8> = Vec::new();

    // Set tempo: 120 BPM = 500000 us/beat
    track.push(0x00); // delta time
    track.extend_from_slice(&[0xFF, 0x51, 0x03]);
    track.push(0x07); // 500000 = 0x07A120
    track.push(0xA1);
    track.push(0x20);

    // Melody: "Ode to Joy" simplified (C major)
    let melody: &[(u8, u16)] = &[
        (64, 480), // E
        (64, 480), // E
        (65, 480), // F
        (67, 480), // G
        (67, 480), // G
        (65, 480), // F
        (64, 480), // E
        (62, 480), // D
        (60, 480), // C
        (60, 480), // C
        (62, 480), // D
        (64, 480), // E
        (64, 720), // E (dotted)
        (62, 240), // D (short)
        (62, 960), // D (half note)
    ];

    for (note, duration) in melody {
        // Note on
        track.push(0x00); // delta = 0
        track.extend_from_slice(&[0x90, *note, 100]);

        // Note off after duration
        write_variable_length(&mut track, *duration as u32);
        track.extend_from_slice(&[0x80, *note, 0]);
    }

    // End of track
    track.push(0x00);
    track.extend_from_slice(&[0xFF, 0x2F, 0x00]);

    data.extend_from_slice(b"MTrk");
    data.extend_from_slice(&(track.len() as u32).to_be_bytes());
    data.extend_from_slice(&track);

    std::fs::write(path, &data).unwrap();
}

fn write_chord_midi(path: &Path) {
    let mut data: Vec<u8> = Vec::new();

    // Header
    data.extend_from_slice(b"MThd");
    data.extend_from_slice(&6u32.to_be_bytes());
    data.extend_from_slice(&0u16.to_be_bytes());
    data.extend_from_slice(&1u16.to_be_bytes());
    data.extend_from_slice(&480u16.to_be_bytes());

    let mut track: Vec<u8> = Vec::new();

    // Tempo: 90 BPM = 666667 us/beat
    track.push(0x00);
    track.extend_from_slice(&[0xFF, 0x51, 0x03]);
    track.push(0x0A);
    track.push(0x2C);
    track.push(0x2B);

    // Chord progression: C - Am - F - G
    let chords: &[(&[u8], u16)] = &[
        (&[60, 64, 67], 1920),     // C major (whole note)
        (&[57, 60, 64], 1920),     // A minor
        (&[53, 57, 60], 1920),     // F major
        (&[55, 59, 62], 1920),     // G major
        (&[60, 64, 67, 72], 3840), // C major (two whole notes, with octave)
    ];

    for (notes, duration) in chords {
        // All notes on simultaneously
        for (i, note) in notes.iter().enumerate() {
            if i > 0 {
                track.push(0x00); // delta = 0 for simultaneous
            } else {
                track.push(0x00);
            }
            track.extend_from_slice(&[0x90, *note, 90]);
        }

        // All notes off after duration
        for (i, note) in notes.iter().enumerate() {
            if i == 0 {
                write_variable_length(&mut track, *duration as u32);
            } else {
                track.push(0x00);
            }
            track.extend_from_slice(&[0x80, *note, 0]);
        }
    }

    // End of track
    track.push(0x00);
    track.extend_from_slice(&[0xFF, 0x2F, 0x00]);

    data.extend_from_slice(b"MTrk");
    data.extend_from_slice(&(track.len() as u32).to_be_bytes());
    data.extend_from_slice(&track);

    std::fs::write(path, &data).unwrap();
}

fn write_variable_length(buf: &mut Vec<u8>, mut value: u32) {
    if value < 128 {
        buf.push(value as u8);
        return;
    }
    let mut bytes = Vec::new();
    bytes.push((value & 0x7F) as u8);
    value >>= 7;
    while value > 0 {
        bytes.push((value & 0x7F) as u8 | 0x80);
        value >>= 7;
    }
    bytes.reverse();
    buf.extend_from_slice(&bytes);
}
