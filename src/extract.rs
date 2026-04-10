//! mame-synth-extract: CLI tool for extracting patches and MIDI from VGM files.
//!
//! Usage:
//!   mame-synth-extract <input.vgm|input.vgz> [--output-dir DIR] [--midi FILE]
//!
//! Extracts:
//!   - Patches (JSON) compatible with mame-synth's patch system
//!   - MIDI note events as a standard MIDI-like sequence

use std::path::PathBuf;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: mame-synth-extract <input.vgm|vgz> [--output-dir DIR]");
        eprintln!();
        eprintln!("Extracts patches and note events from VGM files.");
        eprintln!("Patches are saved as JSON files compatible with mame-synth.");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  --output-dir DIR   Save patches to DIR (default: ./extracted/)");
        eprintln!();
        eprintln!("Supported chips: SN76489, YM2612, YM2151, AY-3-8910, NES APU, POKEY");
        eprintln!();
        eprintln!("VGM files available from:");
        eprintln!("  https://vgmrips.net/packs/");
        eprintln!("  https://www.smspower.org/Music/VGMs");
        eprintln!("  https://project2612.org/");
        std::process::exit(1);
    }

    let input = PathBuf::from(&args[1]);
    let output_dir = args
        .iter()
        .position(|a| a == "--output-dir")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("extracted"));

    if !input.exists() {
        eprintln!("Error: file not found: {}", input.display());
        std::process::exit(1);
    }

    // Handle single file or directory
    let files: Vec<PathBuf> = if input.is_dir() {
        std::fs::read_dir(&input)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .map(|e| e == "vgm" || e == "vgz")
                    .unwrap_or(false)
            })
            .collect()
    } else {
        vec![input.clone()]
    };

    if files.is_empty() {
        eprintln!("No VGM files found.");
        std::process::exit(1);
    }

    std::fs::create_dir_all(&output_dir).ok();
    let mut total_patches = 0;
    let mut total_events = 0;

    for file in &files {
        println!("Processing: {}", file.display());

        match synth_core::vgm::VgmFile::load(file) {
            Ok(vgm) => {
                println!("  {}", vgm.summary().replace('\n', "\n  "));

                let extractions = synth_core::vgm_extract::extract(&vgm);

                for ext in &extractions {
                    println!(
                        "  {} → {} patches, {} events",
                        ext.chip_name,
                        ext.patches.len(),
                        ext.events.len()
                    );

                    // Save patches
                    for patch in &ext.patches {
                        let filename = format!(
                            "{}_{}.json",
                            sanitize(&file.file_stem().unwrap_or_default().to_string_lossy()),
                            sanitize(&patch.name)
                        );
                        let path = output_dir.join(&filename);
                        match patch.save(&path) {
                            Ok(()) => {
                                println!("    Patch: {}", filename);
                                total_patches += 1;
                            }
                            Err(e) => eprintln!("    Error saving patch: {}", e),
                        }
                    }

                    // Print note summary
                    let on_count = ext.events.iter().filter(|e| e.is_on).count();
                    total_events += on_count;
                    if on_count > 0 {
                        let notes: Vec<u8> = ext
                            .events
                            .iter()
                            .filter(|e| e.is_on)
                            .map(|e| e.note)
                            .collect();
                        let min_note = notes.iter().min().copied().unwrap_or(0);
                        let max_note = notes.iter().max().copied().unwrap_or(0);
                        println!(
                            "    Notes: {} events, range MIDI {}-{}, duration {:.1}s",
                            on_count,
                            min_note,
                            max_note,
                            ext.duration_us as f64 / 1_000_000.0
                        );
                    }
                }
            }
            Err(e) => eprintln!("  Error: {}", e),
        }
        println!();
    }

    println!(
        "Done. Extracted {} patches, {} note events from {} files.",
        total_patches,
        total_events,
        files.len()
    );
    println!("Patches saved to: {}", output_dir.display());
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
