//! VGM (Video Game Music) file parser.
//!
//! Parses VGM/VGZ files containing register writes to sound chips.
//! Used to extract patches (register snapshots) and note sequences.
//!
//! Reference: https://vgmrips.net/wiki/VGM_Specification

use std::io::Read;
use std::path::Path;

/// VGM file header.
#[derive(Debug, Clone)]
pub struct VgmHeader {
    pub version: u32,
    pub total_samples: u32,
    pub loop_offset: u32,
    pub loop_samples: u32,
    pub rate: u32,
    pub data_offset: u32,
    /// Chip clocks (Hz). Zero = chip not used.
    pub sn76489_clock: u32,
    pub ym2612_clock: u32,
    pub ym2151_clock: u32,
    pub ay8910_clock: u32,
    pub nes_apu_clock: u32,
    pub pokey_clock: u32,
}

/// A single VGM command parsed from the stream.
#[derive(Debug, Clone)]
pub enum VgmCommand {
    /// SN76489 write (single data byte)
    Sn76489Write { data: u8 },
    /// YM2612 port 0 register write
    Ym2612Port0 { reg: u8, data: u8 },
    /// YM2612 port 1 register write
    Ym2612Port1 { reg: u8, data: u8 },
    /// YM2151 register write
    Ym2151Write { reg: u8, data: u8 },
    /// AY-3-8910 register write
    Ay8910Write { reg: u8, data: u8 },
    /// NES APU register write
    NesApuWrite { reg: u8, data: u8 },
    /// POKEY register write
    PokeyWrite { reg: u8, data: u8 },
    /// Wait N samples (at 44100 Hz)
    Wait { samples: u32 },
    /// End of data
    End,
    /// Unknown/unsupported command (skip)
    Unknown { cmd: u8 },
}

/// Timed command with absolute sample position.
#[derive(Debug, Clone)]
pub struct TimedCommand {
    pub sample_pos: u64,
    pub command: VgmCommand,
}

/// Parsed VGM file.
#[derive(Debug)]
pub struct VgmFile {
    pub header: VgmHeader,
    pub commands: Vec<TimedCommand>,
    /// Which chips are used in this file.
    pub chips_used: Vec<ChipUsed>,
}

#[derive(Debug, Clone)]
pub struct ChipUsed {
    pub name: &'static str,
    pub clock: u32,
    pub write_count: usize,
}

impl VgmFile {
    /// Load a VGM or VGZ file.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read(path)?;
        let data = if path.extension().map(|e| e == "vgz").unwrap_or(false)
            || (raw.len() >= 2 && raw[0] == 0x1F && raw[1] == 0x8B)
        {
            // Gzip compressed
            let mut decoder = flate2::read::GzDecoder::new(&raw[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            decompressed
        } else {
            raw
        };
        Self::parse(&data)
    }

    /// Parse VGM data from a byte slice.
    pub fn parse(data: &[u8]) -> anyhow::Result<Self> {
        if data.len() < 64 {
            anyhow::bail!("VGM file too short");
        }
        if &data[0..4] != b"Vgm " {
            anyhow::bail!("Not a VGM file (bad magic)");
        }

        let header = parse_header(data)?;
        let data_start = if header.data_offset > 0 {
            0x34 + header.data_offset as usize
        } else {
            0x40 // default for older versions
        };

        let mut commands = Vec::new();
        let mut pos = data_start;
        let mut sample_pos: u64 = 0;

        // Counters per chip
        let mut sn_count = 0usize;
        let mut ym2612_count = 0usize;
        let mut ym2151_count = 0usize;
        let mut ay_count = 0usize;
        let mut nes_count = 0usize;
        let mut pokey_count = 0usize;

        while pos < data.len() {
            let cmd = data[pos];
            let (command, advance) = match cmd {
                0x50 => {
                    sn_count += 1;
                    (
                        VgmCommand::Sn76489Write {
                            data: data.get(pos + 1).copied().unwrap_or(0),
                        },
                        2,
                    )
                }
                0x52 => {
                    ym2612_count += 1;
                    (
                        VgmCommand::Ym2612Port0 {
                            reg: data.get(pos + 1).copied().unwrap_or(0),
                            data: data.get(pos + 2).copied().unwrap_or(0),
                        },
                        3,
                    )
                }
                0x53 => {
                    ym2612_count += 1;
                    (
                        VgmCommand::Ym2612Port1 {
                            reg: data.get(pos + 1).copied().unwrap_or(0),
                            data: data.get(pos + 2).copied().unwrap_or(0),
                        },
                        3,
                    )
                }
                0x54 => {
                    ym2151_count += 1;
                    (
                        VgmCommand::Ym2151Write {
                            reg: data.get(pos + 1).copied().unwrap_or(0),
                            data: data.get(pos + 2).copied().unwrap_or(0),
                        },
                        3,
                    )
                }
                0xA0 => {
                    ay_count += 1;
                    (
                        VgmCommand::Ay8910Write {
                            reg: data.get(pos + 1).copied().unwrap_or(0),
                            data: data.get(pos + 2).copied().unwrap_or(0),
                        },
                        3,
                    )
                }
                0xB4 => {
                    nes_count += 1;
                    (
                        VgmCommand::NesApuWrite {
                            reg: data.get(pos + 1).copied().unwrap_or(0),
                            data: data.get(pos + 2).copied().unwrap_or(0),
                        },
                        3,
                    )
                }
                0xBB => {
                    pokey_count += 1;
                    (
                        VgmCommand::PokeyWrite {
                            reg: data.get(pos + 1).copied().unwrap_or(0),
                            data: data.get(pos + 2).copied().unwrap_or(0),
                        },
                        3,
                    )
                }
                0x61 => {
                    let lo = data.get(pos + 1).copied().unwrap_or(0) as u32;
                    let hi = data.get(pos + 2).copied().unwrap_or(0) as u32;
                    let samples = lo | (hi << 8);
                    (VgmCommand::Wait { samples }, 3)
                }
                0x62 => (VgmCommand::Wait { samples: 735 }, 1),
                0x63 => (VgmCommand::Wait { samples: 882 }, 1),
                0x66 => (VgmCommand::End, 1),
                0x70..=0x7F => {
                    let n = (cmd & 0x0F) as u32 + 1;
                    (VgmCommand::Wait { samples: n }, 1)
                }
                // Two-byte commands we don't handle
                0x30..=0x3F => (VgmCommand::Unknown { cmd }, 2),
                0x40..=0x4E => (VgmCommand::Unknown { cmd }, 3),
                0x51 | 0x55..=0x5F => (VgmCommand::Unknown { cmd }, 3),
                0xA1..=0xBF => (VgmCommand::Unknown { cmd }, 3),
                0xC0..=0xDF => (VgmCommand::Unknown { cmd }, 4),
                0xE0..=0xE1 => (VgmCommand::Unknown { cmd }, 5),
                0xE2..=0xFF => (VgmCommand::Unknown { cmd }, 5),
                _ => (VgmCommand::Unknown { cmd }, 1),
            };

            match &command {
                VgmCommand::Wait { samples } => {
                    sample_pos += *samples as u64;
                }
                VgmCommand::End => {
                    commands.push(TimedCommand {
                        sample_pos,
                        command,
                    });
                    break;
                }
                _ => {}
            }

            commands.push(TimedCommand {
                sample_pos,
                command,
            });
            pos += advance;
        }

        let mut chips_used = Vec::new();
        if header.sn76489_clock > 0 && sn_count > 0 {
            chips_used.push(ChipUsed {
                name: "SN76489",
                clock: header.sn76489_clock,
                write_count: sn_count,
            });
        }
        if header.ym2612_clock > 0 && ym2612_count > 0 {
            chips_used.push(ChipUsed {
                name: "YM2612",
                clock: header.ym2612_clock,
                write_count: ym2612_count,
            });
        }
        if header.ym2151_clock > 0 && ym2151_count > 0 {
            chips_used.push(ChipUsed {
                name: "YM2151",
                clock: header.ym2151_clock,
                write_count: ym2151_count,
            });
        }
        if ay_count > 0 {
            chips_used.push(ChipUsed {
                name: "AY-3-8910",
                clock: header.ay8910_clock,
                write_count: ay_count,
            });
        }
        if nes_count > 0 {
            chips_used.push(ChipUsed {
                name: "NES APU",
                clock: header.nes_apu_clock,
                write_count: nes_count,
            });
        }
        if pokey_count > 0 {
            chips_used.push(ChipUsed {
                name: "POKEY",
                clock: header.pokey_clock,
                write_count: pokey_count,
            });
        }

        Ok(VgmFile {
            header,
            commands,
            chips_used,
        })
    }

    /// Get duration in seconds.
    pub fn duration_secs(&self) -> f64 {
        self.header.total_samples as f64 / 44100.0
    }

    /// Summary string.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "VGM v{}.{:02} | {:.1}s | {} commands",
            self.header.version >> 8,
            self.header.version & 0xFF,
            self.duration_secs(),
            self.commands.len()
        );
        for chip in &self.chips_used {
            s += &format!(
                "\n  {} @ {}Hz ({} writes)",
                chip.name, chip.clock, chip.write_count
            );
        }
        s
    }
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    if offset + 4 > data.len() {
        return 0;
    }
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn parse_header(data: &[u8]) -> anyhow::Result<VgmHeader> {
    let version = read_u32_le(data, 0x08);
    let total_samples = read_u32_le(data, 0x18);
    let loop_offset = read_u32_le(data, 0x1C);
    let loop_samples = read_u32_le(data, 0x20);
    let rate = read_u32_le(data, 0x24);
    let data_offset = read_u32_le(data, 0x34);

    let sn76489_clock = read_u32_le(data, 0x0C) & 0x7FFFFFFF;
    let ym2612_clock = read_u32_le(data, 0x2C) & 0x7FFFFFFF;
    let ym2151_clock = read_u32_le(data, 0x30) & 0x7FFFFFFF;

    // Extended header fields (v1.51+)
    let ay8910_clock = if data.len() > 0x78 {
        read_u32_le(data, 0x74) & 0x7FFFFFFF
    } else {
        0
    };
    let nes_apu_clock = if data.len() > 0x84 {
        read_u32_le(data, 0x84) & 0x7FFFFFFF
    } else {
        0
    };
    let pokey_clock = if data.len() > 0xB0 {
        read_u32_le(data, 0xB0) & 0x7FFFFFFF
    } else {
        0
    };

    Ok(VgmHeader {
        version,
        total_samples,
        loop_offset,
        loop_samples,
        rate,
        data_offset,
        sn76489_clock,
        ym2612_clock,
        ym2151_clock,
        ay8910_clock,
        nes_apu_clock,
        pokey_clock,
    })
}

/// Create a synthetic VGM file for testing. Generates a simple melody.
pub fn create_test_vgm_sn76489() -> Vec<u8> {
    let mut data = Vec::new();

    // Header (64 bytes minimum)
    let mut header = [0u8; 256];
    header[0..4].copy_from_slice(b"Vgm ");
    // Version 1.50
    header[8] = 0x50;
    header[9] = 0x01;
    // SN76489 clock: 3579545 Hz
    let clock: u32 = 3_579_545;
    header[0x0C..0x10].copy_from_slice(&clock.to_le_bytes());
    // SN76489 feedback
    header[0x28..0x2A].copy_from_slice(&0x0009u16.to_le_bytes());
    header[0x2A] = 16; // shift register width
                       // Data offset (relative to 0x34)
    let data_offset: u32 = 0x0C; // data starts at 0x40
    header[0x34..0x38].copy_from_slice(&data_offset.to_le_bytes());

    data.extend_from_slice(&header);

    // Command stream: play a C major scale
    // SN76489 register format:
    //   Latch byte: 1 RRR DDDD (bit 7=1, bits 6-4=register, bits 3-0=data low)
    //   Data byte:  0 X DDDDDD (bit 7=0, bits 5-0=data high)
    //
    // Registers: 0=Tone0 freq, 1=Tone0 vol, 2=Tone1 freq, 3=Tone1 vol, etc.
    // Volume: 0=loudest, 15=silent

    let notes_hz: [f64; 8] = [261.6, 293.7, 329.6, 349.2, 392.0, 440.0, 493.9, 523.3];

    for freq_hz in &notes_hz {
        let n = (3_579_545.0 / (32.0 * freq_hz)).round() as u16;
        let low = (n & 0x0F) as u8;
        let high = ((n >> 4) & 0x3F) as u8;

        // Latch tone 0 frequency (register 0), low nibble
        data.push(0x50);
        data.push(0x80 | low); // 1_000_DDDD

        // Data byte: high bits
        data.push(0x50);
        data.push(high); // 0_0_DDDDDD

        // Set volume to 0 (loudest) - register 1
        data.push(0x50);
        data.push(0x90); // 1_001_0000 = tone 0 volume, loudest

        // Wait ~0.25 seconds (11025 samples)
        data.push(0x61);
        data.push((11025 & 0xFF) as u8);
        data.push((11025 >> 8) as u8);

        // Silence
        data.push(0x50);
        data.push(0x90 | 0x0F); // 1_001_1111 = volume 15 (silent)

        // Short gap
        data.push(0x61);
        data.push((2205 & 0xFF) as u8);
        data.push((2205 >> 8) as u8);
    }

    // End
    data.push(0x66);

    // Fix total samples count
    let total_samples: u32 = (11025 + 2205) * 8;
    data[0x18..0x1C].copy_from_slice(&total_samples.to_le_bytes());
    // Fix EOF offset
    let eof = (data.len() - 4) as u32;
    data[0x04..0x08].copy_from_slice(&eof.to_le_bytes());

    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_synthetic_sn76489() {
        let data = create_test_vgm_sn76489();
        let vgm = VgmFile::parse(&data).unwrap();
        assert_eq!(vgm.header.version, 0x150);
        assert_eq!(vgm.chips_used.len(), 1);
        assert_eq!(vgm.chips_used[0].name, "SN76489");
        assert!(vgm.chips_used[0].write_count > 0);
        println!("{}", vgm.summary());
    }

    #[test]
    fn extract_sn76489_writes() {
        let data = create_test_vgm_sn76489();
        let vgm = VgmFile::parse(&data).unwrap();

        let sn_writes: Vec<_> = vgm
            .commands
            .iter()
            .filter_map(|tc| match &tc.command {
                VgmCommand::Sn76489Write { data } => Some((tc.sample_pos, *data)),
                _ => None,
            })
            .collect();

        assert!(
            sn_writes.len() >= 16,
            "Should have note-on + note-off pairs"
        );
        // First write should be a latch byte (bit 7 set)
        assert!(sn_writes[0].1 & 0x80 != 0, "First byte should be a latch");
    }

    #[test]
    fn duration_calculation() {
        let data = create_test_vgm_sn76489();
        let vgm = VgmFile::parse(&data).unwrap();
        let dur = vgm.duration_secs();
        assert!(
            dur > 2.0 && dur < 5.0,
            "Scale should be ~2.4 seconds, got {}",
            dur
        );
    }
}
