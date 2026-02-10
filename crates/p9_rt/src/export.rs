use std::f32::consts::{PI, TAU};
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use p9_core::engine::Engine;
use p9_core::events::{RenderEvent, RenderMode};
use p9_core::model::{SamplerRenderVariant, SynthWaveform};
use p9_core::scheduler::Scheduler;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfflineRenderConfig {
    pub sample_rate_hz: u32,
    pub ppq: u16,
    pub ticks: u64,
}

impl Default for OfflineRenderConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 48_000,
            ppq: 24,
            ticks: 96,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportReport {
    pub sample_rate_hz: u32,
    pub ticks_rendered: u64,
    pub events_rendered: usize,
    pub samples_rendered: u32,
    pub peak_abs_sample: i16,
}

#[derive(Debug)]
pub enum ExportError {
    Io(io::Error),
    InvalidTempo(u16),
    InvalidPpq(u16),
    InvalidTicks(u64),
    DataTooLarge(usize),
}

impl From<io::Error> for ExportError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Copy, Debug)]
struct ActiveVoice {
    track_id: u8,
    note: u8,
    waveform: SynthWaveform,
    mode: VoiceRenderMode,
    send_mfx: f32,
    send_delay: f32,
    send_reverb: f32,
    sampler_variant: SamplerRenderVariant,
    sampler_transient_level: f32,
    sampler_body_level: f32,
    phase: f32,
    phase_inc: f32,
    amplitude: f32,
    elapsed_samples: u32,
    attack_samples: u32,
    release_samples: u32,
    release_progress_samples: u32,
    releasing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VoiceRenderMode {
    Standard,
    SamplerV1,
}

#[derive(Clone, Debug)]
struct RenderFxState {
    delay_line: Vec<f32>,
    delay_index: usize,
    reverb_lp: f32,
}

impl RenderFxState {
    fn new(sample_rate_hz: u32) -> Self {
        let delay_samples = (sample_rate_hz / 8).max(1) as usize;
        Self {
            delay_line: vec![0.0; delay_samples],
            delay_index: 0,
            reverb_lp: 0.0,
        }
    }

    fn process_returns(&mut self, send_mfx: f32, send_delay: f32, send_reverb: f32) -> f32 {
        let mfx = soft_clip(send_mfx * 1.8) * 0.42;

        let delayed = self.delay_line[self.delay_index];
        let delay_input = send_delay + delayed * 0.45;
        self.delay_line[self.delay_index] = delay_input;
        self.delay_index = (self.delay_index + 1) % self.delay_line.len();
        let delay_out = delayed * 0.34;

        self.reverb_lp = self.reverb_lp * 0.82 + send_reverb * 0.18;
        let reverb_out = self.reverb_lp * 0.28;

        (mfx + delay_out + reverb_out).clamp(-1.0, 1.0)
    }
}

pub fn render_project_to_wav(
    engine: &Engine,
    path: impl AsRef<Path>,
    config: OfflineRenderConfig,
) -> Result<ExportReport, ExportError> {
    if config.ppq == 0 {
        return Err(ExportError::InvalidPpq(config.ppq));
    }
    if config.ticks == 0 {
        return Err(ExportError::InvalidTicks(config.ticks));
    }

    let tempo = engine.snapshot().song.tempo;
    if tempo == 0 {
        return Err(ExportError::InvalidTempo(tempo));
    }

    let samples_per_tick = samples_per_tick(config.sample_rate_hz, tempo, config.ppq);
    let mut scheduler = Scheduler::new(config.ppq);
    let mut voices: Vec<ActiveVoice> = Vec::new();
    let mut fx_state = RenderFxState::new(config.sample_rate_hz);
    let mut samples = Vec::<i16>::with_capacity(
        samples_per_tick
            .saturating_mul(config.ticks as usize)
            .max(samples_per_tick),
    );
    let mut events_rendered = 0usize;
    let mut peak_abs_sample = 0i16;

    for _ in 0..config.ticks {
        let events = scheduler.tick(engine);
        events_rendered = events_rendered.saturating_add(events.len());

        for event in &events {
            apply_event(&mut voices, event, config.sample_rate_hz as f32);
        }

        for _ in 0..samples_per_tick {
            let sample = synthesize_sample_routed(&mut voices, &mut fx_state);
            let sample_i16 = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            peak_abs_sample = peak_abs_sample.max(sample_i16.saturating_abs());
            samples.push(sample_i16);
        }
    }

    write_wav_mono_i16(path.as_ref(), config.sample_rate_hz, &samples)?;

    let samples_rendered =
        u32::try_from(samples.len()).map_err(|_| ExportError::DataTooLarge(samples.len()))?;

    Ok(ExportReport {
        sample_rate_hz: config.sample_rate_hz,
        ticks_rendered: config.ticks,
        events_rendered,
        samples_rendered,
        peak_abs_sample,
    })
}

fn samples_per_tick(sample_rate_hz: u32, tempo: u16, ppq: u16) -> usize {
    let ticks_per_second = (tempo as f32 * ppq as f32) / 60.0;
    let per_tick = (sample_rate_hz as f32 / ticks_per_second).round();
    per_tick.max(1.0) as usize
}

fn apply_event(voices: &mut Vec<ActiveVoice>, event: &RenderEvent, sample_rate_hz: f32) {
    match event {
        RenderEvent::NoteOn {
            track_id,
            note,
            velocity,
            render_mode,
            track_level,
            master_level,
            send_mfx,
            send_delay,
            send_reverb,
            sampler_variant,
            sampler_transient_level,
            sampler_body_level,
            waveform,
            attack_ms,
            release_ms,
            gain,
            ..
        } => {
            voices.retain(|voice| !(voice.track_id == *track_id && voice.note == *note));

            if *gain == 0 || matches!(render_mode, RenderMode::ExternalMuted) {
                return;
            }

            let freq_hz = 440.0 * 2.0_f32.powf((*note as f32 - 69.0) / 12.0);
            let phase_inc = TAU * (freq_hz / sample_rate_hz.max(1.0));
            let velocity_gain = *velocity as f32 / 127.0;
            let instrument_gain = *gain as f32 / 127.0;
            let track_gain = (*track_level as f32 / 127.0).clamp(0.0, 1.0);
            let master_gain = (*master_level as f32 / 127.0).clamp(0.0, 1.0);
            let mode = match render_mode {
                RenderMode::SamplerV1 => VoiceRenderMode::SamplerV1,
                RenderMode::Synth | RenderMode::ExternalMuted => VoiceRenderMode::Standard,
            };
            let mode_gain = match mode {
                VoiceRenderMode::Standard => 0.22,
                VoiceRenderMode::SamplerV1 => 0.28,
            };

            voices.push(ActiveVoice {
                track_id: *track_id,
                note: *note,
                waveform: *waveform,
                mode,
                send_mfx: (*send_mfx as f32 / 127.0).clamp(0.0, 1.0),
                send_delay: (*send_delay as f32 / 127.0).clamp(0.0, 1.0),
                send_reverb: (*send_reverb as f32 / 127.0).clamp(0.0, 1.0),
                sampler_variant: *sampler_variant,
                sampler_transient_level: *sampler_transient_level as f32 / 127.0,
                sampler_body_level: *sampler_body_level as f32 / 127.0,
                phase: 0.0,
                phase_inc,
                amplitude: (velocity_gain * instrument_gain * track_gain * master_gain * mode_gain)
                    .clamp(0.0, 1.0),
                elapsed_samples: 0,
                attack_samples: ms_to_samples(*attack_ms, sample_rate_hz),
                release_samples: ms_to_samples(*release_ms, sample_rate_hz),
                release_progress_samples: 0,
                releasing: false,
            });
        }
        RenderEvent::NoteOff { track_id, note } => {
            for voice in voices.iter_mut() {
                if voice.track_id == *track_id && voice.note == *note {
                    voice.releasing = true;
                    voice.release_progress_samples = 0;
                }
            }
        }
    }
}

#[cfg(test)]
fn synthesize_sample(voices: &mut Vec<ActiveVoice>) -> f32 {
    if voices.is_empty() {
        return 0.0;
    }

    let mut mixed = 0.0f32;

    for voice in voices.iter_mut() {
        let osc = oscillator_sample(voice);
        let env = envelope_sample(voice);
        mixed += osc * voice.amplitude * env;
        voice.phase += voice.phase_inc;
        if voice.phase >= TAU {
            voice.phase -= TAU;
        }
        voice.elapsed_samples = voice.elapsed_samples.saturating_add(1);
        if voice.releasing && voice.release_samples > 0 {
            voice.release_progress_samples = voice.release_progress_samples.saturating_add(1);
        }
    }

    voices.retain(|voice| {
        if !voice.releasing {
            return true;
        }

        if voice.release_samples == 0 {
            return false;
        }

        voice.release_progress_samples < voice.release_samples
    });

    mixed
}

fn synthesize_sample_routed(voices: &mut Vec<ActiveVoice>, fx_state: &mut RenderFxState) -> f32 {
    if voices.is_empty() {
        return fx_state.process_returns(0.0, 0.0, 0.0);
    }

    let mut dry_mixed = 0.0f32;
    let mut send_mfx = 0.0f32;
    let mut send_delay = 0.0f32;
    let mut send_reverb = 0.0f32;

    for voice in voices.iter_mut() {
        let osc = oscillator_sample(voice);
        let env = envelope_sample(voice);
        let sample = osc * voice.amplitude * env;

        let total_send = (voice.send_mfx + voice.send_delay + voice.send_reverb).clamp(0.0, 1.0);
        let dry_scale = (1.0 - total_send * 0.6).clamp(0.4, 1.0);
        dry_mixed += sample * dry_scale;
        send_mfx += sample * voice.send_mfx;
        send_delay += sample * voice.send_delay;
        send_reverb += sample * voice.send_reverb;

        voice.phase += voice.phase_inc;
        if voice.phase >= TAU {
            voice.phase -= TAU;
        }
        voice.elapsed_samples = voice.elapsed_samples.saturating_add(1);
        if voice.releasing && voice.release_samples > 0 {
            voice.release_progress_samples = voice.release_progress_samples.saturating_add(1);
        }
    }

    voices.retain(|voice| {
        if !voice.releasing {
            return true;
        }

        if voice.release_samples == 0 {
            return false;
        }

        voice.release_progress_samples < voice.release_samples
    });

    let returns = fx_state.process_returns(send_mfx, send_delay, send_reverb);
    (dry_mixed + returns).clamp(-1.0, 1.0)
}

fn oscillator_sample(voice: &ActiveVoice) -> f32 {
    match voice.mode {
        VoiceRenderMode::Standard => waveform_sample(voice.waveform, voice.phase),
        VoiceRenderMode::SamplerV1 => {
            let base = waveform_sample(voice.waveform, voice.phase);
            let sine = voice.phase.sin();
            let (variant_base_mix, variant_sine_mix, variant_transient_scale) = match voice
                .sampler_variant
            {
                SamplerRenderVariant::Classic => (0.65, 0.35, 1.0),
                SamplerRenderVariant::Punch => (0.58, 0.42, 1.25),
                SamplerRenderVariant::Air => (0.76, 0.24, 0.85),
            };
            let body_mix = voice.sampler_body_level.clamp(0.0, 1.0);
            let transient_mix = voice.sampler_transient_level.clamp(0.0, 1.0);
            let base_weight = (variant_base_mix * body_mix).clamp(0.0, 1.0);
            let sine_weight = (variant_sine_mix * (1.0 - body_mix * 0.5)).clamp(0.0, 1.0);
            let weight_sum = (base_weight + sine_weight).max(1e-6);
            let body = ((base * base_weight) + (sine * sine_weight)) / weight_sum;
            let transient_window = (1.0 - (voice.elapsed_samples as f32 / 96.0)).clamp(0.0, 1.0);
            let transient = transient_window
                * ((voice.phase * 2.0).sin().abs() * 2.0 - 1.0)
                * transient_mix
                * variant_transient_scale;
            (body + transient * 0.25).clamp(-1.0, 1.0)
        }
    }
}

fn waveform_sample(waveform: SynthWaveform, phase: f32) -> f32 {
    match waveform {
        SynthWaveform::Sine => phase.sin(),
        SynthWaveform::Square => {
            if phase.sin() >= 0.0 {
                1.0
            } else {
                -1.0
            }
        }
        SynthWaveform::Saw => (phase / PI) - 1.0,
        SynthWaveform::Triangle => {
            let normalized = phase / TAU;
            2.0 * (2.0 * (normalized - (normalized + 0.5).floor())).abs() - 1.0
        }
    }
}

fn envelope_sample(voice: &ActiveVoice) -> f32 {
    let attack_env = if voice.attack_samples == 0 {
        1.0
    } else {
        (voice.elapsed_samples as f32 / voice.attack_samples as f32).clamp(0.0, 1.0)
    };

    let release_env = if !voice.releasing {
        1.0
    } else if voice.release_samples == 0 {
        0.0
    } else {
        (1.0 - (voice.release_progress_samples as f32 / voice.release_samples as f32))
            .clamp(0.0, 1.0)
    };

    attack_env * release_env
}

fn ms_to_samples(ms: u16, sample_rate_hz: f32) -> u32 {
    if ms == 0 {
        return 0;
    }

    let samples = ((ms as f32 / 1000.0) * sample_rate_hz).round();
    samples.max(1.0) as u32
}

fn soft_clip(value: f32) -> f32 {
    value / (1.0 + value.abs())
}

fn write_wav_mono_i16(path: &Path, sample_rate_hz: u32, samples: &[i16]) -> Result<(), ExportError> {
    let data_len = samples
        .len()
        .checked_mul(2)
        .ok_or(ExportError::DataTooLarge(samples.len()))?;
    let data_len_u32 = u32::try_from(data_len).map_err(|_| ExportError::DataTooLarge(samples.len()))?;
    let riff_size = 36u32.saturating_add(data_len_u32);

    let mut file = File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&riff_size.to_le_bytes())?;
    file.write_all(b"WAVE")?;

    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&sample_rate_hz.to_le_bytes())?;

    let byte_rate = sample_rate_hz.saturating_mul(2);
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;

    file.write_all(b"data")?;
    file.write_all(&data_len_u32.to_le_bytes())?;

    for sample in samples {
        file.write_all(&sample.to_le_bytes())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        apply_event, render_project_to_wav, synthesize_sample, synthesize_sample_routed,
        OfflineRenderConfig, RenderFxState,
    };
    use p9_core::engine::{Engine, EngineCommand};
    use p9_core::events::{RenderEvent, RenderMode};
    use p9_core::model::{Chain, Instrument, InstrumentType, Phrase};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn setup_engine() -> Engine {
        let mut engine = Engine::new("export-test");

        let mut chain = Chain::new(0);
        chain.rows[0].phrase_id = Some(0);
        engine
            .apply_command(EngineCommand::UpsertChain { chain })
            .unwrap();

        let mut phrase = Phrase::new(0);
        phrase.steps[0].note = Some(60);
        phrase.steps[0].velocity = 100;
        phrase.steps[4].note = Some(64);
        phrase.steps[4].velocity = 100;
        engine
            .apply_command(EngineCommand::UpsertPhrase { phrase })
            .unwrap();

        engine
            .apply_command(EngineCommand::SetSongRowChain {
                track_index: 0,
                row: 0,
                chain_id: Some(0),
            })
            .unwrap();

        engine
    }

    fn temp_file(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let mut path = std::env::temp_dir();
        path.push(format!("{}_{}_{}.wav", prefix, std::process::id(), nanos));
        path
    }

    #[test]
    fn render_project_to_wav_writes_valid_riff_file() {
        let engine = setup_engine();
        let output = temp_file("p9_export_valid");

        let report = render_project_to_wav(
            &engine,
            &output,
            OfflineRenderConfig {
                ticks: 48,
                ..OfflineRenderConfig::default()
            },
        )
        .unwrap();

        let bytes = fs::read(&output).unwrap();
        assert!(bytes.starts_with(b"RIFF"));
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[36..40], b"data");
        assert!(report.events_rendered > 0);
        assert!(report.samples_rendered > 0);
        assert!(report.peak_abs_sample > 0);
        assert!(bytes.len() > 44);

        let _ = fs::remove_file(output);
    }

    #[test]
    fn render_project_to_wav_is_deterministic() {
        let engine = setup_engine();
        let left_path = temp_file("p9_export_det_l");
        let right_path = temp_file("p9_export_det_r");
        let cfg = OfflineRenderConfig {
            ticks: 64,
            ..OfflineRenderConfig::default()
        };

        let left = render_project_to_wav(&engine, &left_path, cfg).unwrap();
        let right = render_project_to_wav(&engine, &right_path, cfg).unwrap();

        let left_bytes = fs::read(&left_path).unwrap();
        let right_bytes = fs::read(&right_path).unwrap();

        assert_eq!(left, right);
        assert_eq!(left_bytes, right_bytes);

        let _ = fs::remove_file(left_path);
        let _ = fs::remove_file(right_path);
    }

    #[test]
    fn render_project_to_wav_midiout_profile_is_silent() {
        let mut engine = setup_engine();
        let instrument = Instrument::new(0, InstrumentType::MidiOut, "MIDI Out");
        engine
            .apply_command(EngineCommand::UpsertInstrument { instrument })
            .unwrap();
        for (step, note, velocity) in [(0usize, Some(60u8), 100u8), (4usize, Some(64u8), 100u8)] {
            engine
                .apply_command(EngineCommand::SetPhraseStep {
                    phrase_id: 0,
                    step_index: step,
                    note,
                    velocity,
                    instrument_id: Some(0),
                })
                .unwrap();
        }

        let output = temp_file("p9_export_midiout_silent");
        let report = render_project_to_wav(
            &engine,
            &output,
            OfflineRenderConfig {
                ticks: 48,
                ..OfflineRenderConfig::default()
            },
        )
        .unwrap();

        assert_eq!(report.peak_abs_sample, 0);
        let _ = fs::remove_file(output);
    }

    #[test]
    fn render_project_sampler_profile_differs_from_synth_profile() {
        let mut synth_engine = setup_engine();
        let synth = Instrument::new(0, InstrumentType::Synth, "Synth");
        synth_engine
            .apply_command(EngineCommand::UpsertInstrument { instrument: synth })
            .unwrap();
        synth_engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 0,
                note: Some(60),
                velocity: 100,
                instrument_id: Some(0),
            })
            .unwrap();
        synth_engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 4,
                note: None,
                velocity: 100,
                instrument_id: Some(0),
            })
            .unwrap();

        let mut sampler_engine = setup_engine();
        let sampler = Instrument::new(0, InstrumentType::Sampler, "Sampler");
        sampler_engine
            .apply_command(EngineCommand::UpsertInstrument { instrument: sampler })
            .unwrap();
        sampler_engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 0,
                note: Some(60),
                velocity: 100,
                instrument_id: Some(0),
            })
            .unwrap();
        sampler_engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 4,
                note: None,
                velocity: 100,
                instrument_id: Some(0),
            })
            .unwrap();

        let synth_path = temp_file("p9_export_synth_profile");
        let sampler_path = temp_file("p9_export_sampler_profile");
        let cfg = OfflineRenderConfig {
            ticks: 48,
            ..OfflineRenderConfig::default()
        };

        let synth_report = render_project_to_wav(&synth_engine, &synth_path, cfg).unwrap();
        let sampler_report = render_project_to_wav(&sampler_engine, &sampler_path, cfg).unwrap();
        let synth_bytes = fs::read(&synth_path).unwrap();
        let sampler_bytes = fs::read(&sampler_path).unwrap();

        assert!(synth_report.peak_abs_sample > 0);
        assert!(sampler_report.peak_abs_sample > 0);
        assert_ne!(synth_bytes, sampler_bytes);

        let _ = fs::remove_file(synth_path);
        let _ = fs::remove_file(sampler_path);
    }

    #[test]
    fn external_render_mode_mutes_even_with_nonzero_gain() {
        let mut voices = Vec::new();
        let event = RenderEvent::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 110,
            render_mode: RenderMode::ExternalMuted,
            track_level: 127,
            master_level: 127,
            send_mfx: 0,
            send_delay: 0,
            send_reverb: 0,
            instrument_id: Some(0),
            waveform: p9_core::model::SynthWaveform::Saw,
            attack_ms: 5,
            release_ms: 80,
            gain: 100,
            sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
            sampler_transient_level: 64,
            sampler_body_level: 96,
        };

        apply_event(&mut voices, &event, 48_000.0);
        let sample = synthesize_sample(&mut voices);

        assert!(voices.is_empty());
        assert_eq!(sample, 0.0);
    }

    #[test]
    fn sampler_render_mode_is_not_heuristic() {
        let mut synth_voices = Vec::new();
        let mut sampler_voices = Vec::new();

        let synth_event = RenderEvent::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 100,
            render_mode: RenderMode::Synth,
            track_level: 127,
            master_level: 127,
            send_mfx: 0,
            send_delay: 0,
            send_reverb: 0,
            instrument_id: Some(0),
            waveform: p9_core::model::SynthWaveform::Saw,
            attack_ms: 9,
            release_ms: 9,
            gain: 100,
            sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
            sampler_transient_level: 64,
            sampler_body_level: 96,
        };
        let sampler_event = RenderEvent::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 100,
            render_mode: RenderMode::SamplerV1,
            track_level: 127,
            master_level: 127,
            send_mfx: 0,
            send_delay: 0,
            send_reverb: 0,
            instrument_id: Some(0),
            waveform: p9_core::model::SynthWaveform::Saw,
            attack_ms: 9,
            release_ms: 9,
            gain: 100,
            sampler_variant: p9_core::model::SamplerRenderVariant::Punch,
            sampler_transient_level: 110,
            sampler_body_level: 40,
        };

        apply_event(&mut synth_voices, &synth_event, 48_000.0);
        apply_event(&mut sampler_voices, &sampler_event, 48_000.0);

        let mut synth_energy = 0.0f32;
        let mut sampler_energy = 0.0f32;
        for _ in 0..32 {
            synth_energy += synthesize_sample(&mut synth_voices).abs();
            sampler_energy += synthesize_sample(&mut sampler_voices).abs();
        }

        assert!(!synth_voices.is_empty());
        assert!(!sampler_voices.is_empty());
        assert_ne!(synth_energy, sampler_energy);
    }

    #[test]
    fn mixer_levels_scale_export_energy() {
        let mut full_mix_voices = Vec::new();
        let mut muted_mix_voices = Vec::new();
        let mut full_fx = RenderFxState::new(48_000);
        let mut muted_fx = RenderFxState::new(48_000);

        apply_event(
            &mut full_mix_voices,
            &RenderEvent::NoteOn {
                track_id: 0,
                note: 60,
                velocity: 110,
                render_mode: RenderMode::Synth,
                track_level: 127,
                master_level: 127,
                send_mfx: 0,
                send_delay: 0,
                send_reverb: 0,
                instrument_id: Some(0),
                waveform: p9_core::model::SynthWaveform::Saw,
                attack_ms: 1,
                release_ms: 48,
                gain: 100,
                sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
                sampler_transient_level: 64,
                sampler_body_level: 96,
            },
            48_000.0,
        );

        apply_event(
            &mut muted_mix_voices,
            &RenderEvent::NoteOn {
                track_id: 0,
                note: 60,
                velocity: 110,
                render_mode: RenderMode::Synth,
                track_level: 0,
                master_level: 127,
                send_mfx: 0,
                send_delay: 0,
                send_reverb: 0,
                instrument_id: Some(0),
                waveform: p9_core::model::SynthWaveform::Saw,
                attack_ms: 1,
                release_ms: 48,
                gain: 100,
                sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
                sampler_transient_level: 64,
                sampler_body_level: 96,
            },
            48_000.0,
        );

        let mut full_energy = 0.0f32;
        let mut muted_energy = 0.0f32;
        for _ in 0..64 {
            full_energy += synthesize_sample_routed(&mut full_mix_voices, &mut full_fx).abs();
            muted_energy += synthesize_sample_routed(&mut muted_mix_voices, &mut muted_fx).abs();
        }

        assert!(full_energy > 0.1);
        assert!(muted_energy < full_energy * 0.05);
    }

    #[test]
    fn send_routing_changes_export_signature() {
        fn render_signature(send_mfx: u8, send_delay: u8, send_reverb: u8) -> i64 {
            let mut voices = Vec::new();
            let mut fx = RenderFxState::new(48_000);
            apply_event(
                &mut voices,
                &RenderEvent::NoteOn {
                    track_id: 0,
                    note: 60,
                    velocity: 112,
                    render_mode: RenderMode::Synth,
                    track_level: 127,
                    master_level: 127,
                    send_mfx,
                    send_delay,
                    send_reverb,
                    instrument_id: Some(0),
                    waveform: p9_core::model::SynthWaveform::Saw,
                    attack_ms: 1,
                    release_ms: 56,
                    gain: 100,
                    sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
                    sampler_transient_level: 64,
                    sampler_body_level: 96,
                },
                48_000.0,
            );

            let mut signature = 0.0f64;
            for frame in 0u32..192 {
                let sample = synthesize_sample_routed(&mut voices, &mut fx);
                signature += sample as f64 * (frame + 1) as f64;
            }

            (signature * 1_000_000.0).round() as i64
        }

        let dry = render_signature(0, 0, 0);
        let routed = render_signature(96, 64, 48);
        assert_ne!(dry, routed);
    }

    #[test]
    fn long_mixed_mode_session_is_deterministic_and_variant_sensitive() {
        fn mixed_session_signature(
            sampler_variant: p9_core::model::SamplerRenderVariant,
            sampler_transient_level: u8,
            sampler_body_level: u8,
        ) -> (i64, i16) {
            let mut voices = Vec::new();
            let mut signature = 0.0f64;
            let mut peak = 0.0f32;

            for tick in 0u32..128 {
                if tick % 4 == 0 {
                    apply_event(
                        &mut voices,
                        &RenderEvent::NoteOn {
                            track_id: 0,
                            note: 48 + (tick % 12) as u8,
                            velocity: 100,
                            render_mode: RenderMode::Synth,
                            track_level: 127,
                            master_level: 127,
                            send_mfx: 0,
                            send_delay: 0,
                            send_reverb: 0,
                            instrument_id: Some(0),
                            waveform: p9_core::model::SynthWaveform::Saw,
                            attack_ms: 5,
                            release_ms: 64,
                            gain: 100,
                            sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
                            sampler_transient_level: 64,
                            sampler_body_level: 96,
                        },
                        48_000.0,
                    );
                }

                if tick % 4 == 2 {
                    apply_event(
                        &mut voices,
                        &RenderEvent::NoteOff {
                            track_id: 0,
                            note: 48 + (tick % 12) as u8,
                        },
                        48_000.0,
                    );
                }

                if tick % 5 == 0 {
                    apply_event(
                        &mut voices,
                        &RenderEvent::NoteOn {
                            track_id: 1,
                            note: 60 + (tick % 7) as u8,
                            velocity: 108,
                            render_mode: RenderMode::SamplerV1,
                            track_level: 127,
                            master_level: 127,
                            send_mfx: 0,
                            send_delay: 0,
                            send_reverb: 0,
                            instrument_id: Some(1),
                            waveform: p9_core::model::SynthWaveform::Saw,
                            attack_ms: 1,
                            release_ms: 48,
                            gain: 100,
                            sampler_variant,
                            sampler_transient_level,
                            sampler_body_level,
                        },
                        48_000.0,
                    );
                }

                if tick % 5 == 3 {
                    apply_event(
                        &mut voices,
                        &RenderEvent::NoteOff {
                            track_id: 1,
                            note: 60 + (tick % 7) as u8,
                        },
                        48_000.0,
                    );
                }

                if tick % 7 == 0 {
                    apply_event(
                        &mut voices,
                        &RenderEvent::NoteOn {
                            track_id: 2,
                            note: 36,
                            velocity: 120,
                            render_mode: RenderMode::ExternalMuted,
                            track_level: 127,
                            master_level: 127,
                            send_mfx: 0,
                            send_delay: 0,
                            send_reverb: 0,
                            instrument_id: Some(2),
                            waveform: p9_core::model::SynthWaveform::Square,
                            attack_ms: 1,
                            release_ms: 1,
                            gain: 110,
                            sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
                            sampler_transient_level: 64,
                            sampler_body_level: 64,
                        },
                        48_000.0,
                    );
                }

                if tick % 7 == 1 {
                    apply_event(
                        &mut voices,
                        &RenderEvent::NoteOff {
                            track_id: 2,
                            note: 36,
                        },
                        48_000.0,
                    );
                }

                for frame in 0u32..24 {
                    let sample = synthesize_sample(&mut voices);
                    let weight = (tick * 24 + frame + 1) as f64;
                    signature += sample as f64 * weight;
                    peak = peak.max(sample.abs());
                }
            }

            (((signature * 1_000_000.0).round() as i64), (peak * i16::MAX as f32) as i16)
        }

        let baseline = mixed_session_signature(
            p9_core::model::SamplerRenderVariant::Punch,
            112,
            44,
        );
        let repeat = mixed_session_signature(
            p9_core::model::SamplerRenderVariant::Punch,
            112,
            44,
        );
        let altered = mixed_session_signature(p9_core::model::SamplerRenderVariant::Air, 52, 116);

        assert_eq!(baseline, repeat);
        assert_ne!(baseline, altered);
        assert!(baseline.1 > 0);
    }
}
