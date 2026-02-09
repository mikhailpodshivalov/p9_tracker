use std::f32::consts::{PI, TAU};
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use p9_core::engine::Engine;
use p9_core::events::RenderEvent;
use p9_core::model::SynthWaveform;
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
    phase: f32,
    phase_inc: f32,
    amplitude: f32,
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
            let sample = synthesize_sample(&mut voices);
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
            waveform,
            gain,
            ..
        } => {
            voices.retain(|voice| !(voice.track_id == *track_id && voice.note == *note));

            let freq_hz = 440.0 * 2.0_f32.powf((*note as f32 - 69.0) / 12.0);
            let phase_inc = TAU * (freq_hz / sample_rate_hz.max(1.0));
            let velocity_gain = *velocity as f32 / 127.0;
            let instrument_gain = *gain as f32 / 127.0;

            voices.push(ActiveVoice {
                track_id: *track_id,
                note: *note,
                waveform: *waveform,
                phase: 0.0,
                phase_inc,
                amplitude: (velocity_gain * instrument_gain * 0.22).clamp(0.0, 1.0),
            });
        }
        RenderEvent::NoteOff { track_id, note } => {
            voices.retain(|voice| !(voice.track_id == *track_id && voice.note == *note));
        }
    }
}

fn synthesize_sample(voices: &mut [ActiveVoice]) -> f32 {
    if voices.is_empty() {
        return 0.0;
    }

    let mut mixed = 0.0f32;

    for voice in voices {
        let osc = match voice.waveform {
            SynthWaveform::Sine => voice.phase.sin(),
            SynthWaveform::Square => {
                if voice.phase.sin() >= 0.0 {
                    1.0
                } else {
                    -1.0
                }
            }
            SynthWaveform::Saw => (voice.phase / PI) - 1.0,
            SynthWaveform::Triangle => {
                let normalized = voice.phase / TAU;
                2.0 * (2.0 * (normalized - (normalized + 0.5).floor())).abs() - 1.0
            }
        };

        mixed += osc * voice.amplitude;
        voice.phase += voice.phase_inc;
        if voice.phase >= TAU {
            voice.phase -= TAU;
        }
    }

    mixed
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
    use super::{render_project_to_wav, OfflineRenderConfig};
    use p9_core::engine::{Engine, EngineCommand};
    use p9_core::model::{Chain, Phrase};
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
}
