use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer};

use crate::ay8910::Ay8910;
use crate::chip::{ChipId, StereoSample};
use crate::messages::{AudioMessage, GuiMessage};
use crate::pokey::Pokey;
use crate::ricoh2a03::Ricoh2a03;
use crate::sid6581::Sid6581;
use crate::sn76489::Sn76489;
use crate::voice::ChipBank;
use crate::ym2151::Ym2151;
use crate::ym2612::Ym2612;

struct AudioState {
    banks: Vec<ChipBank>,
    active_bank_index: usize,
    consumer: Consumer<AudioMessage>,
    gui_producer: Producer<GuiMessage>,
    sample_buffer: Vec<StereoSample>,
    sample_counter: u32,
    peak_left: f32,
    peak_right: f32,
}

pub struct AudioEngine {
    _stream: cpal::Stream,
    sample_rate: u32,
}

fn create_bank(chip_id: ChipId, count: usize, sample_rate: u32) -> ChipBank {
    let chips: Vec<Box<dyn crate::chip::SoundChip>> = (0..count)
        .map(|_| -> Box<dyn crate::chip::SoundChip> {
            match chip_id {
                ChipId::Sn76489 => Box::new(Sn76489::new(sample_rate)),
                ChipId::Ym2612 => Box::new(Ym2612::new(sample_rate)),
                ChipId::Sid6581 => Box::new(Sid6581::new(sample_rate)),
                ChipId::Ay8910 => Box::new(Ay8910::new(sample_rate)),
                ChipId::Ricoh2a03 => Box::new(Ricoh2a03::new(sample_rate)),
                ChipId::Pokey => Box::new(Pokey::new(sample_rate)),
                ChipId::Ym2151 => Box::new(Ym2151::new(sample_rate)),
            }
        })
        .collect();
    ChipBank::new(chips)
}

impl AudioEngine {
    pub fn new(
        msg_consumer: Consumer<AudioMessage>,
        gui_producer: Producer<GuiMessage>,
    ) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No audio output device found"))?;

        let default_config = device.default_output_config()?;
        let sample_rate = default_config.sample_rate().0;

        log::info!(
            "Audio device: {}, sample rate: {}, format: {:?}",
            device.name().unwrap_or_default(),
            sample_rate,
            default_config.sample_format()
        );

        let config = cpal::StreamConfig {
            channels: 2,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let mut state = AudioState {
            banks: ChipId::all()
                .iter()
                .map(|id| create_bank(*id, 1, sample_rate))
                .collect(),
            active_bank_index: 0,
            consumer: msg_consumer,
            gui_producer,
            sample_buffer: vec![StereoSample::default(); 8192],
            sample_counter: 0,
            peak_left: 0.0,
            peak_right: 0.0,
        };

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                audio_callback(&mut state, data);
            },
            |err| eprintln!("[AUDIO ERROR] {}", err),
            None,
        )?;

        stream.play()?;
        log::info!("Audio stream started");

        Ok(AudioEngine {
            _stream: stream,
            sample_rate,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

fn audio_callback(state: &mut AudioState, output: &mut [f32]) {
    // Drain all pending messages (non-blocking)
    while let Ok(msg) = state.consumer.pop() {
        match msg {
            AudioMessage::SwitchChip(id) => {
                if let Some(idx) = state.banks.iter().position(|b| b.chip_id() == id) {
                    state.banks[state.active_bank_index].reset();
                    state.active_bank_index = idx;
                }
            }
            AudioMessage::SetParam { param_id, value } => {
                state.banks[state.active_bank_index].set_param(param_id, value);
            }
            AudioMessage::NoteOn { note, velocity } => {
                state.banks[state.active_bank_index].note_on(note, velocity);
            }
            AudioMessage::NoteOff { note } => {
                state.banks[state.active_bank_index].note_off(note);
            }
            AudioMessage::Reset => {
                state.banks[state.active_bank_index].reset();
            }
            AudioMessage::SetVoiceMode(mode) => {
                state.banks[state.active_bank_index].set_voice_mode(mode);
            }
            AudioMessage::PitchBend { .. } => {}
        }
    }

    // Generate samples
    let num_frames = output.len() / 2;
    if state.sample_buffer.len() < num_frames {
        state
            .sample_buffer
            .resize(num_frames, StereoSample::default());
    }

    let buf = &mut state.sample_buffer[..num_frames];
    state.banks[state.active_bank_index].generate_samples(buf);

    // Interleave into output and track peaks
    for (i, sample) in buf.iter().enumerate() {
        output[i * 2] = sample.left;
        output[i * 2 + 1] = sample.right;
        state.peak_left = state.peak_left.max(sample.left.abs());
        state.peak_right = state.peak_right.max(sample.right.abs());
    }

    // Send peak levels to GUI periodically
    state.sample_counter += num_frames as u32;
    if state.sample_counter >= 4096 {
        let _ = state.gui_producer.push(GuiMessage::PeakLevel {
            left: state.peak_left,
            right: state.peak_right,
        });
        state.peak_left = 0.0;
        state.peak_right = 0.0;
        state.sample_counter = 0;
    }
}
