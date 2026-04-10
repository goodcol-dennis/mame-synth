use synth_core::audio::AudioEngine;
use synth_core::messages::{AudioMessage, GuiMessage};
use synth_gui::app::MameSynthApp;

fn main() -> eframe::Result<()> {
    env_logger::init();

    // Request low-latency audio from PipeWire (256 samples ≈ 5.3ms at 48kHz)
    // This only affects our process — doesn't change system-wide settings.
    // PipeWire respects this when using the ALSA compatibility layer.
    std::env::set_var("PIPEWIRE_QUANTUM", "256/48000");

    // Create rtrb ring buffers for thread communication
    let (audio_tx, audio_rx) = rtrb::RingBuffer::<AudioMessage>::new(1024);
    let (gui_tx, gui_rx) = rtrb::RingBuffer::<GuiMessage>::new(256);

    // Start the audio engine (opens cpal stream immediately)
    let _audio_engine = AudioEngine::new(audio_rx, gui_tx).expect("Failed to start audio engine");

    log::info!("Audio engine started");

    // Build the GUI app
    let app = MameSynthApp::new(audio_tx, gui_rx);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([850.0, 700.0])
            .with_title("mame-synth"),
        ..Default::default()
    };

    eframe::run_native("mame-synth", options, Box::new(|_cc| Ok(Box::new(app))))
}
