//! Headless Wayland E2E tests for mame-synth.
//!
//! Launches mame-synth inside a headless `cage` compositor.  Commands are
//! delivered by writing to /tmp/mame-synth-input.txt, which the app polls
//! on every frame — no key injection required.
//!
//! ## Requirements
//!   sudo apt install cage
//!   (wtype / wlrctl are only needed for the optional keyboard / mouse helpers)
//!
//! ## Run
//!   cargo test --release --test e2e_wayland -- --nocapture --test-threads=1

use std::collections::{HashMap, HashSet};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const WARMUP: Duration = Duration::from_millis(1500);
/// How long to wait for the app to process a command written to the input file.
/// The app renders at ~60 fps (16 ms/frame), so two frames is plenty, but we
/// add extra headroom for CI scheduling jitter.
const CMD_DELAY: Duration = Duration::from_millis(200);
/// How long to wait after a state-dump request before reading the output file.
const DUMP_TIMEOUT: Duration = Duration::from_secs(3);
const SHORT: Duration = Duration::from_millis(200);

// ── HeadlessSession ──

struct HeadlessSession {
    cage_child: Child,
    wayland_display: String,
}

impl HeadlessSession {
    fn start() -> Self {
        let runtime_dir = runtime_dir();
        let before = list_wayland_sockets(&runtime_dir);

        let bin = env!("CARGO_BIN_EXE_mame-synth");
        let cage_child = Command::new("cage")
            .args(["-d", "--", bin])
            .env("WLR_BACKENDS", "headless")
            .env("WLR_LIBINPUT_NO_DEVICES", "1")
            .env("WAYLAND_DISPLAY", "") // don't nest on parent
            .env("RUST_LOG", "info,zbus=warn,sctk=warn,tracing=warn")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to start cage (sudo apt install cage)");

        let start = std::time::Instant::now();
        let mut socket = String::new();
        while start.elapsed() < Duration::from_secs(5) {
            let after = list_wayland_sockets(&runtime_dir);
            if let Some(s) = after.difference(&before).next() {
                socket = s.clone();
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(!socket.is_empty(), "cage did not create a Wayland socket");

        // Give cage + egui time to paint the first frame.
        std::thread::sleep(Duration::from_secs(3));

        let session = Self {
            cage_child,
            wayland_display: socket,
        };

        // Clean up any leftover input file from a previous run.
        let _ = std::fs::remove_file("/tmp/mame-synth-input.txt");
        std::thread::sleep(WARMUP);

        session
    }

    fn wtype_raw(&self, args: &[&str]) -> bool {
        Command::new("wtype")
            .args(args)
            .env("WAYLAND_DISPLAY", &self.wayland_display)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn wlrctl(&self, args: &[&str]) -> bool {
        Command::new("wlrctl")
            .args(args)
            .env("WAYLAND_DISPLAY", &self.wayland_display)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn send_command(&self, cmd: &str) {
        // Write the command; the app polls this file on every frame and
        // deletes it once consumed — no key injection needed.
        std::fs::write("/tmp/mame-synth-input.txt", cmd).unwrap();

        // Wait until the file is gone (consumed) so the next command never
        // overwrites an unread one.  Fall back to CMD_DELAY if the app is
        // slow or the file lingers for any other reason.
        let deadline = std::time::Instant::now() + CMD_DELAY;
        while std::time::Instant::now() < deadline {
            if !std::path::Path::new("/tmp/mame-synth-input.txt").exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn switch_chip(&self, name: &str) {
        self.send_command(&format!("switch-chip {name}"));
    }

    fn note_on(&self, note: u8, vel: u8) {
        self.send_command(&format!("note-on {note} {vel}"));
    }

    fn note_off(&self, note: u8) {
        self.send_command(&format!("note-off {note}"));
    }

    fn set_param(&self, id: u32, val: f32) {
        self.send_command(&format!("set-param {id} {val}"));
    }

    fn set_voice_mode(&self, mode: &str) {
        self.send_command(&format!("set-voice-mode {mode}"));
    }

    #[allow(dead_code)]
    fn reset(&self) {
        self.send_command("reset");
    }

    fn dump(&self) -> State {
        // Remove stale state file, then ask the app to write a fresh one.
        let _ = std::fs::remove_file("/tmp/mame-synth-state.txt");
        std::fs::write("/tmp/mame-synth-input.txt", "dump-state").unwrap();

        // Poll until the state file appears (or we time out).
        let deadline = std::time::Instant::now() + DUMP_TIMEOUT;
        loop {
            if std::path::Path::new("/tmp/mame-synth-state.txt").exists() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                eprintln!("WARN: timed out waiting for state dump");
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        State::load()
    }

    fn key(&self, key: &str) {
        self.wtype_raw(&["-k", key]);
        std::thread::sleep(SHORT);
    }

    #[allow(dead_code)]
    fn mouse_click(&self) {
        self.wlrctl(&["pointer", "click", "left"]);
        std::thread::sleep(SHORT);
    }

    #[allow(dead_code)]
    fn mouse_move(&self, dx: i32, dy: i32) {
        self.wlrctl(&["pointer", "move", &dx.to_string(), &dy.to_string()]);
        std::thread::sleep(SHORT);
    }
}

impl Drop for HeadlessSession {
    fn drop(&mut self) {
        let _ = self.cage_child.kill();
        let _ = self.cage_child.wait();
        let rd = runtime_dir();
        let sp = format!("{rd}/{}", self.wayland_display);
        let _ = std::fs::remove_file(&sp);
        let _ = std::fs::remove_file(format!("{sp}.lock"));
        let _ = std::fs::remove_file("/tmp/mame-synth-input.txt");
        let _ = std::fs::remove_file("/tmp/mame-synth-state.txt");
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[derive(Debug, Default)]
struct State {
    chip: String,
    voice_mode: String,
    unison_detune: f32,
    octave: u8,
    held_keys: Vec<u8>,
    peak_left: f32,
    peak_right: f32,
    num_params: usize,
    params: HashMap<u32, f32>,
}

impl State {
    fn load() -> Self {
        let text = std::fs::read_to_string("/tmp/mame-synth-state.txt").unwrap_or_default();
        let mut s = State::default();
        for line in text.lines() {
            if let Some((key, val)) = line.split_once('=') {
                match key {
                    "chip" => s.chip = val.to_string(),
                    "voice_mode" => s.voice_mode = val.to_string(),
                    "unison_detune" => s.unison_detune = val.parse().unwrap_or(0.0),
                    "octave" => s.octave = val.parse().unwrap_or(4),
                    "held_keys" => {
                        s.held_keys = val
                            .split(',')
                            .filter(|s| !s.is_empty())
                            .filter_map(|s| s.parse().ok())
                            .collect();
                    }
                    "peak_left" => s.peak_left = val.parse().unwrap_or(0.0),
                    "peak_right" => s.peak_right = val.parse().unwrap_or(0.0),
                    "num_params" => s.num_params = val.parse().unwrap_or(0),
                    k if k.starts_with("param_") => {
                        if let Some(id_str) = k.strip_prefix("param_") {
                            if let Ok(id) = id_str.parse::<u32>() {
                                s.params.insert(id, val.parse().unwrap_or(0.0));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        s
    }
}

fn runtime_dir() -> String {
    std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }))
}

fn list_wayland_sockets(dir: &str) -> HashSet<String> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("wayland-") && !name.ends_with(".lock") {
                Some(name)
            } else {
                None
            }
        })
        .collect()
}

// =============================================================================
// All e2e tests run in a single session to avoid cage socket conflicts.
// Each test is a function called from the main test, with the session reused.
// =============================================================================

#[test]
fn e2e_all() {
    let s = HeadlessSession::start();

    // P0 — Core
    p0_app_launches(&s);
    p0_chip_switching(&s);
    p0_note_on_off(&s);
    p0_keyboard_input(&s);

    // P1 — Voice modes
    p1_voice_modes(&s);

    // P2 — Parameters
    p2_set_param(&s);
    p2_chip_switch_reloads_params(&s);

    // P3 — Stress
    p3_rapid_chip_switching(&s);
    p3_note_spam(&s);

    eprintln!("All e2e tests passed!");
}

fn p0_app_launches(s: &HeadlessSession) {
    let state = s.dump();
    assert_eq!(
        state.chip, "SN76489 (PSG)",
        "Default chip should be SN76489"
    );
    assert_eq!(state.voice_mode, "poly");
    assert!(state.num_params > 0, "Should have parameters");
    eprintln!("  p0_app_launches ... ok");
}

fn p0_chip_switching(s: &HeadlessSession) {
    s.switch_chip("ym2612");
    let state = s.dump();
    assert_eq!(state.chip, "YM2612 (FM)");

    s.switch_chip("sid6581");
    let state = s.dump();
    assert_eq!(state.chip, "SID 6581 (C64)");

    s.switch_chip("sn76489");
    let state = s.dump();
    assert_eq!(state.chip, "SN76489 (PSG)");
    eprintln!("  p0_chip_switching ... ok");
}

fn p0_note_on_off(s: &HeadlessSession) {
    s.note_on(60, 100);
    let state = s.dump();
    assert!(state.held_keys.contains(&60), "Note 60 should be held");

    s.note_off(60);
    let state = s.dump();
    assert!(!state.held_keys.contains(&60), "Note 60 should be released");
    eprintln!("  p0_note_on_off ... ok");
}

fn p0_keyboard_input(s: &HeadlessSession) {
    s.key("z");
    std::thread::sleep(Duration::from_millis(300));
    let state = s.dump();
    assert!(
        !state.chip.is_empty(),
        "App should still be running after keypress"
    );
    eprintln!("  p0_keyboard_input ... ok");
}

fn p1_voice_modes(s: &HeadlessSession) {
    s.set_voice_mode("mono");
    let state = s.dump();
    assert_eq!(state.voice_mode, "mono");

    s.set_voice_mode("unison 20.0");
    let state = s.dump();
    assert_eq!(state.voice_mode, "unison");
    assert!((state.unison_detune - 20.0).abs() < 0.1);

    s.set_voice_mode("poly");
    let state = s.dump();
    assert_eq!(state.voice_mode, "poly");
    eprintln!("  p1_voice_modes ... ok");
}

fn p2_set_param(s: &HeadlessSession) {
    // Ensure we're on SN76489
    s.switch_chip("sn76489");
    s.set_param(0, 7.0);
    let state = s.dump();
    let val = state.params.get(&0).copied().unwrap_or(-1.0);
    assert!(
        (val - 7.0).abs() < 0.1,
        "Param 0 should be 7.0, got {}",
        val
    );
    eprintln!("  p2_set_param ... ok");
}

fn p2_chip_switch_reloads_params(s: &HeadlessSession) {
    s.switch_chip("sn76489");
    let sn_state = s.dump();
    let sn_params = sn_state.num_params;

    s.switch_chip("sid6581");
    let sid_state = s.dump();
    assert_ne!(
        sn_params, sid_state.num_params,
        "Different chips should have different param counts (SN76489={}, SID={})",
        sn_params, sid_state.num_params
    );

    // Reset to SN76489
    s.switch_chip("sn76489");
    eprintln!("  p2_chip_switch_reloads_params ... ok");
}

fn p3_rapid_chip_switching(s: &HeadlessSession) {
    for _ in 0..5 {
        s.switch_chip("ym2612");
        s.switch_chip("sid6581");
        s.switch_chip("sn76489");
    }
    let state = s.dump();
    assert_eq!(state.chip, "SN76489 (PSG)");
    eprintln!("  p3_rapid_chip_switching ... ok");
}

fn p3_note_spam(s: &HeadlessSession) {
    for note in [36, 48, 55, 60, 64, 67, 72, 79, 84] {
        s.note_on(note, 127);
    }
    let state = s.dump();
    assert!(!state.held_keys.is_empty(), "Some notes should be held");

    for note in [36, 48, 55, 60, 64, 67, 72, 79, 84] {
        s.note_off(note);
    }
    let state = s.dump();
    assert!(state.held_keys.is_empty(), "All notes should be released");
    eprintln!("  p3_note_spam ... ok");
}
