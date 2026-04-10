use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::chip::{ChipId, VoiceMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub name: String,
    pub chip: String,
    pub voice_mode: String,
    #[serde(default)]
    pub unison_detune: f32,
    pub params: HashMap<String, f32>,
}

impl Patch {
    pub fn from_state(
        name: &str,
        chip_id: ChipId,
        voice_mode: VoiceMode,
        param_ids: &[u32],
        param_values: &[f32],
    ) -> Self {
        let mut params = HashMap::new();
        for (id, val) in param_ids.iter().zip(param_values.iter()) {
            params.insert(id.to_string(), *val);
        }
        Patch {
            name: name.to_string(),
            chip: chip_id_to_str(chip_id).to_string(),
            voice_mode: voice_mode_to_str(voice_mode).to_string(),
            unison_detune: match voice_mode {
                VoiceMode::Unison { detune_cents } => detune_cents,
                _ => 0.0,
            },
            params,
        }
    }

    pub fn chip_id(&self) -> Option<ChipId> {
        str_to_chip_id(&self.chip)
    }

    pub fn voice_mode(&self) -> VoiceMode {
        match self.voice_mode.as_str() {
            "mono" => VoiceMode::Mono,
            "unison" => VoiceMode::Unison {
                detune_cents: self.unison_detune,
            },
            _ => VoiceMode::Poly,
        }
    }

    pub fn get_param(&self, id: u32) -> Option<f32> {
        self.params.get(&id.to_string()).copied()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let patch: Patch = serde_json::from_str(&json)?;
        Ok(patch)
    }
}

fn chip_id_to_str(id: ChipId) -> &'static str {
    match id {
        ChipId::Sn76489 => "sn76489",
        ChipId::Ym2612 => "ym2612",
        ChipId::Sid6581 => "sid6581",
        ChipId::Ay8910 => "ay8910",
        ChipId::Ricoh2a03 => "2a03",
        ChipId::Pokey => "pokey",
        ChipId::Ym2151 => "ym2151",
        ChipId::Ym3812 => "ym3812",
        ChipId::Ymf262 => "ymf262",
        ChipId::Scc => "scc",
        ChipId::NamcoWsg => "namco_wsg",
    }
}

fn str_to_chip_id(s: &str) -> Option<ChipId> {
    match s {
        "sn76489" => Some(ChipId::Sn76489),
        "ym2612" => Some(ChipId::Ym2612),
        "sid6581" => Some(ChipId::Sid6581),
        "ay8910" => Some(ChipId::Ay8910),
        "2a03" => Some(ChipId::Ricoh2a03),
        "pokey" => Some(ChipId::Pokey),
        "ym2151" => Some(ChipId::Ym2151),
        "ym3812" => Some(ChipId::Ym3812),
        "ymf262" => Some(ChipId::Ymf262),
        "scc" => Some(ChipId::Scc),
        "namco_wsg" => Some(ChipId::NamcoWsg),
        _ => None,
    }
}

fn voice_mode_to_str(mode: VoiceMode) -> &'static str {
    match mode {
        VoiceMode::Poly => "poly",
        VoiceMode::Mono => "mono",
        VoiceMode::Unison { .. } => "unison",
    }
}

/// Manages patch storage and provides factory presets.
pub struct PatchBank {
    patches_dir: PathBuf,
    patches: Vec<(String, PathBuf)>, // (name, path) sorted by name
}

impl PatchBank {
    pub fn new(patches_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&patches_dir).ok();
        let mut bank = PatchBank {
            patches_dir,
            patches: Vec::new(),
        };
        bank.scan();
        bank
    }

    pub fn scan(&mut self) {
        self.patches.clear();
        if let Ok(entries) = std::fs::read_dir(&self.patches_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(patch) = Patch::load(&path) {
                        self.patches.push((patch.name.clone(), path));
                    }
                }
            }
        }
        self.patches.sort_by(|a, b| a.0.cmp(&b.0));
    }

    pub fn list(&self) -> &[(String, PathBuf)] {
        &self.patches
    }

    pub fn save_patch(&mut self, patch: &Patch) -> anyhow::Result<PathBuf> {
        let filename = sanitize_filename(&patch.name);
        let path = self.patches_dir.join(format!("{}.json", filename));
        patch.save(&path)?;
        self.scan();
        Ok(path)
    }

    pub fn load_patch(&self, index: usize) -> Option<Patch> {
        self.patches
            .get(index)
            .and_then(|(_, path)| Patch::load(path).ok())
    }

    pub fn delete_patch(&mut self, index: usize) -> anyhow::Result<()> {
        if let Some((_, path)) = self.patches.get(index) {
            std::fs::remove_file(path)?;
            self.scan();
        }
        Ok(())
    }

    /// Write factory presets if the patches directory is empty.
    pub fn ensure_factory_presets(&mut self) {
        if !self.patches.is_empty() {
            return;
        }
        for preset in factory_presets() {
            self.save_patch(&preset).ok();
        }
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn factory_presets() -> Vec<Patch> {
    vec![
        // SN76489 presets
        Patch {
            name: "PSG Square Lead".into(),
            chip: "sn76489".into(),
            voice_mode: "mono".into(),
            unison_detune: 0.0,
            params: HashMap::from([
                ("0".into(), 0.0),  // tone 1 vol: loud
                ("1".into(), 15.0), // tone 2 vol: silent
                ("2".into(), 15.0), // tone 3 vol: silent
                ("3".into(), 15.0), // noise vol: silent
            ]),
        },
        Patch {
            name: "PSG Chord".into(),
            chip: "sn76489".into(),
            voice_mode: "poly".into(),
            unison_detune: 0.0,
            params: HashMap::from([
                ("0".into(), 0.0),
                ("1".into(), 0.0),
                ("2".into(), 0.0),
                ("3".into(), 15.0),
            ]),
        },
        // SID presets
        Patch {
            name: "C64 Bass".into(),
            chip: "sid6581".into(),
            voice_mode: "mono".into(),
            unison_detune: 0.0,
            params: HashMap::from([
                ("0".into(), 1.0),    // sawtooth
                ("1".into(), 2048.0), // pulse width
                ("2".into(), 0.0),    // fast attack
                ("3".into(), 6.0),    // medium decay
                ("4".into(), 8.0),    // mid sustain
                ("5".into(), 4.0),    // medium release
                ("6".into(), 15.0),   // full volume
            ]),
        },
        Patch {
            name: "C64 Fat Unison".into(),
            chip: "sid6581".into(),
            voice_mode: "unison".into(),
            unison_detune: 12.0,
            params: HashMap::from([
                ("0".into(), 1.0), // sawtooth
                ("1".into(), 2048.0),
                ("2".into(), 2.0),
                ("3".into(), 4.0),
                ("4".into(), 10.0),
                ("5".into(), 6.0),
                ("6".into(), 15.0),
            ]),
        },
        Patch {
            name: "C64 Pulse Lead".into(),
            chip: "sid6581".into(),
            voice_mode: "mono".into(),
            unison_detune: 0.0,
            params: HashMap::from([
                ("0".into(), 2.0),    // pulse
                ("1".into(), 1200.0), // narrow pulse
                ("2".into(), 0.0),
                ("3".into(), 3.0),
                ("4".into(), 12.0),
                ("5".into(), 3.0),
                ("6".into(), 15.0),
            ]),
        },
        // YM2612 presets
        Patch {
            name: "FM Electric Piano".into(),
            chip: "ym2612".into(),
            voice_mode: "poly".into(),
            unison_detune: 0.0,
            params: HashMap::from([
                ("0".into(), 4.0),    // algorithm 4
                ("1".into(), 3.0),    // feedback 3
                ("100".into(), 40.0), // op1 TL
                ("101".into(), 31.0), // op1 AR
                ("102".into(), 8.0),  // op1 D1R
                ("104".into(), 6.0),  // op1 SL
                ("105".into(), 7.0),  // op1 RR
                ("106".into(), 2.0),  // op1 MUL
                ("300".into(), 0.0),  // op4 TL (carrier)
                ("301".into(), 31.0),
                ("302".into(), 5.0),
                ("304".into(), 4.0),
                ("305".into(), 5.0),
                ("306".into(), 1.0),
            ]),
        },
        Patch {
            name: "FM Brass".into(),
            chip: "ym2612".into(),
            voice_mode: "poly".into(),
            unison_detune: 0.0,
            params: HashMap::from([
                ("0".into(), 0.0),    // algorithm 0 (serial)
                ("1".into(), 5.0),    // feedback 5
                ("100".into(), 30.0), // op1 TL
                ("101".into(), 31.0),
                ("106".into(), 1.0),  // op1 MUL
                ("200".into(), 35.0), // op2 TL
                ("201".into(), 28.0),
                ("206".into(), 3.0),
                ("300".into(), 0.0), // op4 TL
                ("301".into(), 31.0),
                ("302".into(), 4.0),
                ("304".into(), 3.0),
                ("305".into(), 6.0),
                ("306".into(), 1.0),
            ]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_round_trip() {
        let patch = Patch {
            name: "Test Patch".into(),
            chip: "sid6581".into(),
            voice_mode: "mono".into(),
            unison_detune: 0.0,
            params: HashMap::from([("0".into(), 1.0), ("2".into(), 5.0)]),
        };

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.json");
        patch.save(&path).unwrap();

        let loaded = Patch::load(&path).unwrap();
        assert_eq!(loaded.name, "Test Patch");
        assert_eq!(loaded.chip, "sid6581");
        assert_eq!(loaded.get_param(0), Some(1.0));
        assert_eq!(loaded.get_param(2), Some(5.0));
        assert_eq!(loaded.get_param(99), None);
    }

    #[test]
    fn patch_bank_save_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let mut bank = PatchBank::new(dir.path().to_path_buf());
        assert!(bank.list().is_empty());

        let patch = Patch {
            name: "My Patch".into(),
            chip: "sn76489".into(),
            voice_mode: "poly".into(),
            unison_detune: 0.0,
            params: HashMap::new(),
        };
        bank.save_patch(&patch).unwrap();
        assert_eq!(bank.list().len(), 1);
        assert_eq!(bank.list()[0].0, "My Patch");
    }

    #[test]
    fn factory_presets_created_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mut bank = PatchBank::new(dir.path().to_path_buf());
        bank.ensure_factory_presets();
        assert!(bank.list().len() >= 5, "Should have factory presets");
    }

    #[test]
    fn from_state_round_trip() {
        let chip = ChipId::Sid6581;
        let mode = VoiceMode::Unison { detune_cents: 15.0 };
        let ids = vec![0, 1, 2];
        let vals = vec![1.0, 2048.0, 3.0];
        let patch = Patch::from_state("test", chip, mode, &ids, &vals);
        assert_eq!(patch.chip_id(), Some(ChipId::Sid6581));
        assert_eq!(patch.get_param(0), Some(1.0));
        assert_eq!(patch.get_param(1), Some(2048.0));
        assert!(matches!(patch.voice_mode(), VoiceMode::Unison { .. }));
    }
}
