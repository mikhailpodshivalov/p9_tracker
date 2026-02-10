use std::collections::HashMap;

use p9_core::model::{
    Chain, FxCommand, Groove, Instrument, InstrumentType, ProjectData, SamplerRenderVariant,
    Scale, SynthWaveform, Table, CHAIN_ROW_COUNT, PHRASE_STEP_COUNT, SONG_ROW_COUNT, TRACK_COUNT,
};

pub const FORMAT_VERSION: u16 = 2;
const FORMAT_VERSION_V1: u16 = 1;
const FX_SLOT_COUNT: usize = 3;

#[derive(Clone, Debug)]
pub struct ProjectEnvelope {
    pub format_version: u16,
    pub project: ProjectData,
}

#[derive(Clone, Debug)]
pub enum StorageError {
    UnsupportedFormat(u16),
    MissingField(&'static str),
    ParseError(String),
    InvalidIndex(&'static str, usize),
}

#[derive(Clone, Debug, Default)]
struct ChainRowPatch {
    phrase_id: Option<Option<u8>>,
    transpose: Option<i8>,
}

#[derive(Clone, Debug, Default)]
struct StepPatch {
    note: Option<Option<u8>>,
    velocity: Option<u8>,
    instrument_id: Option<Option<u8>>,
    fx: HashMap<usize, Option<FxCommand>>,
}

#[derive(Clone, Debug, Default)]
struct ScalePatch {
    key: Option<u8>,
    mask: Option<u16>,
}

#[derive(Clone, Debug, Default)]
struct InstrumentPatch {
    instrument_type: Option<InstrumentType>,
    name: Option<String>,
    table_id: Option<Option<u8>>,
    note_length_steps: Option<u8>,
    send_mfx: Option<u8>,
    send_delay: Option<u8>,
    send_reverb: Option<u8>,
    synth_waveform: Option<SynthWaveform>,
    synth_attack_ms: Option<u16>,
    synth_release_ms: Option<u16>,
    synth_gain: Option<u8>,
    sampler_variant: Option<SamplerRenderVariant>,
    sampler_transient_level: Option<u8>,
    sampler_body_level: Option<u8>,
}

#[derive(Clone, Debug, Default)]
struct TableRowPatch {
    note_offset: Option<i8>,
    volume: Option<u8>,
    fx: HashMap<usize, Option<FxCommand>>,
}

#[derive(Clone, Debug, Default)]
struct MixerPatch {
    track_levels: HashMap<usize, u8>,
    master_level: Option<u8>,
    send_mfx: Option<u8>,
    send_delay: Option<u8>,
    send_reverb: Option<u8>,
}

impl ProjectEnvelope {
    pub fn new(project: ProjectData) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            project,
        }
    }

    pub fn validate_format(&self) -> Result<(), StorageError> {
        if self.format_version != FORMAT_VERSION {
            return Err(StorageError::UnsupportedFormat(self.format_version));
        }
        Ok(())
    }

    pub fn to_text(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!("format_version={}", FORMAT_VERSION));
        lines.push(format!("song.name={}", self.project.song.name));
        lines.push(format!("song.tempo={}", self.project.song.tempo));
        lines.push(format!(
            "song.default_groove={}",
            self.project.song.default_groove
        ));
        lines.push(format!(
            "song.default_scale={}",
            self.project.song.default_scale
        ));

        for (track_idx, track) in self.project.song.tracks.iter().enumerate() {
            lines.push(format!(
                "track.{}.mute={}",
                track_idx,
                if track.mute { 1 } else { 0 }
            ));
            lines.push(format!(
                "track.{}.solo={}",
                track_idx,
                if track.solo { 1 } else { 0 }
            ));
            lines.push(format!(
                "track.{}.groove_override={}",
                track_idx,
                render_opt_u8(track.groove_override)
            ));
            lines.push(format!(
                "track.{}.scale_override={}",
                track_idx,
                render_opt_u8(track.scale_override)
            ));

            for (row, slot) in track.song_rows.iter().enumerate() {
                if let Some(chain_id) = slot {
                    lines.push(format!("track.{}.row.{}.chain={}", track_idx, row, chain_id));
                }
            }
        }

        let mut chain_ids: Vec<_> = self.project.chains.keys().copied().collect();
        chain_ids.sort_unstable();
        for chain_id in chain_ids {
            if let Some(chain) = self.project.chains.get(&chain_id) {
                for row in 0..chain.rows.len() {
                    let chain_row = &chain.rows[row];
                    if chain_row.phrase_id.is_some() {
                        lines.push(format!(
                            "chain.{}.row.{}.phrase={}",
                            chain_id,
                            row,
                            render_opt_u8(chain_row.phrase_id)
                        ));
                    }
                    if chain_row.transpose != 0 {
                        lines.push(format!(
                            "chain.{}.row.{}.transpose={}",
                            chain_id, row, chain_row.transpose
                        ));
                    }
                }
            }
        }

        let mut phrase_ids: Vec<_> = self.project.phrases.keys().copied().collect();
        phrase_ids.sort_unstable();
        for phrase_id in phrase_ids {
            if let Some(phrase) = self.project.phrases.get(&phrase_id) {
                for step_idx in 0..phrase.steps.len() {
                    let step = &phrase.steps[step_idx];

                    if step.note.is_some() {
                        lines.push(format!(
                            "phrase.{}.step.{}.note={}",
                            phrase_id,
                            step_idx,
                            render_opt_u8(step.note)
                        ));
                    }
                    if step.velocity != 0x40 {
                        lines.push(format!(
                            "phrase.{}.step.{}.velocity={}",
                            phrase_id, step_idx, step.velocity
                        ));
                    }
                    if step.instrument_id.is_some() {
                        lines.push(format!(
                            "phrase.{}.step.{}.instrument={}",
                            phrase_id,
                            step_idx,
                            render_opt_u8(step.instrument_id)
                        ));
                    }

                    for slot in 0..step.fx.len() {
                        if let Some(command) = &step.fx[slot] {
                            lines.push(format!(
                                "phrase.{}.step.{}.fx.{}={}",
                                phrase_id,
                                step_idx,
                                slot,
                                render_fx_command(command)
                            ));
                        }
                    }
                }
            }
        }

        let mut instrument_ids: Vec<_> = self.project.instruments.keys().copied().collect();
        instrument_ids.sort_unstable();
        for instrument_id in instrument_ids {
            if let Some(instrument) = self.project.instruments.get(&instrument_id) {
                lines.push(format!(
                    "instrument.{}.type={}",
                    instrument_id,
                    render_instrument_type(&instrument.instrument_type)
                ));
                lines.push(format!("instrument.{}.name={}", instrument_id, instrument.name));
                lines.push(format!(
                    "instrument.{}.table={}",
                    instrument_id,
                    render_opt_u8(instrument.table_id)
                ));
                lines.push(format!(
                    "instrument.{}.note_length_steps={}",
                    instrument_id, instrument.note_length_steps
                ));
                lines.push(format!(
                    "instrument.{}.send.mfx={}",
                    instrument_id, instrument.send_levels.mfx
                ));
                lines.push(format!(
                    "instrument.{}.send.delay={}",
                    instrument_id, instrument.send_levels.delay
                ));
                lines.push(format!(
                    "instrument.{}.send.reverb={}",
                    instrument_id, instrument.send_levels.reverb
                ));
                lines.push(format!(
                    "instrument.{}.synth.waveform={}",
                    instrument_id,
                    render_waveform(instrument.synth_params.waveform)
                ));
                lines.push(format!(
                    "instrument.{}.synth.attack_ms={}",
                    instrument_id, instrument.synth_params.attack_ms
                ));
                lines.push(format!(
                    "instrument.{}.synth.release_ms={}",
                    instrument_id, instrument.synth_params.release_ms
                ));
                lines.push(format!(
                    "instrument.{}.synth.gain={}",
                    instrument_id, instrument.synth_params.gain
                ));
                if let Some(sampler_render) = instrument.sampler_render {
                    lines.push(format!(
                        "instrument.{}.sampler.variant={}",
                        instrument_id,
                        render_sampler_variant(sampler_render.variant)
                    ));
                    lines.push(format!(
                        "instrument.{}.sampler.transient_level={}",
                        instrument_id, sampler_render.transient_level
                    ));
                    lines.push(format!(
                        "instrument.{}.sampler.body_level={}",
                        instrument_id, sampler_render.body_level
                    ));
                }
            }
        }

        let mut table_ids: Vec<_> = self.project.tables.keys().copied().collect();
        table_ids.sort_unstable();
        for table_id in table_ids {
            if let Some(table) = self.project.tables.get(&table_id) {
                for row_idx in 0..table.rows.len() {
                    let row = &table.rows[row_idx];
                    if row.note_offset != 0 {
                        lines.push(format!(
                            "table.{}.row.{}.note_offset={}",
                            table_id, row_idx, row.note_offset
                        ));
                    }
                    if row.volume != 0x40 {
                        lines.push(format!(
                            "table.{}.row.{}.volume={}",
                            table_id, row_idx, row.volume
                        ));
                    }
                    for slot in 0..row.fx.len() {
                        if let Some(command) = &row.fx[slot] {
                            lines.push(format!(
                                "table.{}.row.{}.fx.{}={}",
                                table_id,
                                row_idx,
                                slot,
                                render_fx_command(command)
                            ));
                        }
                    }
                }
            }
        }

        let mut groove_ids: Vec<_> = self.project.grooves.keys().copied().collect();
        groove_ids.sort_unstable();
        for groove_id in groove_ids {
            if let Some(groove) = self.project.grooves.get(&groove_id) {
                let values = groove
                    .ticks_pattern
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                lines.push(format!("groove.{}={}", groove_id, values));
            }
        }

        let mut scale_ids: Vec<_> = self.project.scales.keys().copied().collect();
        scale_ids.sort_unstable();
        for scale_id in scale_ids {
            if let Some(scale) = self.project.scales.get(&scale_id) {
                lines.push(format!("scale.{}.key={}", scale_id, scale.key));
                lines.push(format!("scale.{}.mask={}", scale_id, scale.interval_mask));
            }
        }

        for track_idx in 0..TRACK_COUNT {
            lines.push(format!(
                "mixer.track.{}.level={}",
                track_idx, self.project.mixer.track_levels[track_idx]
            ));
        }
        lines.push(format!("mixer.master.level={}", self.project.mixer.master_level));
        lines.push(format!(
            "mixer.send.mfx={}",
            self.project.mixer.send_levels.mfx
        ));
        lines.push(format!(
            "mixer.send.delay={}",
            self.project.mixer.send_levels.delay
        ));
        lines.push(format!(
            "mixer.send.reverb={}",
            self.project.mixer.send_levels.reverb
        ));

        lines.join("\n") + "\n"
    }

    pub fn from_text(input: &str) -> Result<Self, StorageError> {
        let mut source_format_version = None;
        let mut song_name = None;
        let mut tempo = None;
        let mut default_groove = None;
        let mut default_scale = None;

        let mut track_mute: HashMap<usize, bool> = HashMap::new();
        let mut track_solo: HashMap<usize, bool> = HashMap::new();
        let mut track_groove_override: HashMap<usize, Option<u8>> = HashMap::new();
        let mut track_scale_override: HashMap<usize, Option<u8>> = HashMap::new();
        let mut song_rows: HashMap<(usize, usize), u8> = HashMap::new();

        let mut chain_patches: HashMap<(u8, usize), ChainRowPatch> = HashMap::new();
        let mut phrase_patches: HashMap<(u8, usize), StepPatch> = HashMap::new();
        let mut groove_map: HashMap<u8, Vec<u8>> = HashMap::new();
        let mut scale_patches: HashMap<u8, ScalePatch> = HashMap::new();
        let mut instrument_patches: HashMap<u8, InstrumentPatch> = HashMap::new();
        let mut table_row_patches: HashMap<(u8, usize), TableRowPatch> = HashMap::new();
        let mut mixer_patch = MixerPatch::default();

        for line in input.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let Some((key_raw, value_raw)) = trimmed.split_once('=') else {
                continue;
            };
            let key = key_raw.trim();
            let value = value_raw.trim();

            match key {
                "format_version" => {
                    source_format_version = Some(parse_u16(value, "format_version")?);
                    continue;
                }
                "name" | "song.name" => {
                    song_name = Some(value.to_string());
                    continue;
                }
                "tempo" | "song.tempo" => {
                    tempo = Some(parse_u16(value, "song.tempo")?);
                    continue;
                }
                "song.default_groove" => {
                    default_groove = Some(parse_u8(value, "song.default_groove")?);
                    continue;
                }
                "song.default_scale" => {
                    default_scale = Some(parse_u8(value, "song.default_scale")?);
                    continue;
                }
                _ => {}
            }

            if let Some((track_idx, field)) = parse_track_field(key)? {
                match field {
                    TrackField::Mute => {
                        track_mute.insert(track_idx, parse_bool(value, "track.mute")?);
                    }
                    TrackField::Solo => {
                        track_solo.insert(track_idx, parse_bool(value, "track.solo")?);
                    }
                    TrackField::GrooveOverride => {
                        track_groove_override.insert(
                            track_idx,
                            parse_opt_u8(value, "track.groove_override")?,
                        );
                    }
                    TrackField::ScaleOverride => {
                        track_scale_override.insert(
                            track_idx,
                            parse_opt_u8(value, "track.scale_override")?,
                        );
                    }
                    TrackField::SongRowChain(row) => {
                        let chain_id = parse_u8(value, "track.row.chain")?;
                        song_rows.insert((track_idx, row), chain_id);
                    }
                }
                continue;
            }

            if let Some((chain_id, row, field)) = parse_chain_field(key)? {
                let patch = chain_patches.entry((chain_id, row)).or_default();
                match field {
                    ChainField::Phrase => {
                        patch.phrase_id = Some(parse_opt_u8(value, "chain.row.phrase")?)
                    }
                    ChainField::Transpose => {
                        patch.transpose = Some(parse_i8(value, "chain.row.transpose")?)
                    }
                }
                continue;
            }

            if let Some((phrase_id, step, field)) = parse_phrase_field(key)? {
                let patch = phrase_patches.entry((phrase_id, step)).or_default();
                match field {
                    PhraseField::Note => patch.note = Some(parse_opt_u8(value, "phrase.step.note")?),
                    PhraseField::Velocity => {
                        patch.velocity = Some(parse_u8(value, "phrase.step.velocity")?)
                    }
                    PhraseField::Instrument => {
                        patch.instrument_id = Some(parse_opt_u8(value, "phrase.step.instrument")?)
                    }
                    PhraseField::Fx(slot) => {
                        if slot >= FX_SLOT_COUNT {
                            return Err(StorageError::InvalidIndex("fx_slot", slot));
                        }
                        patch
                            .fx
                            .insert(slot, Some(parse_fx_command(value, "phrase.step.fx")?));
                    }
                }
                continue;
            }

            if let Some((instrument_id, field)) = parse_instrument_field(key)? {
                let patch = instrument_patches.entry(instrument_id).or_default();
                match field {
                    InstrumentField::Type => {
                        patch.instrument_type = Some(parse_instrument_type(value)?);
                    }
                    InstrumentField::Name => {
                        patch.name = Some(value.to_string());
                    }
                    InstrumentField::Table => {
                        patch.table_id = Some(parse_opt_u8(value, "instrument.table")?);
                    }
                    InstrumentField::NoteLengthSteps => {
                        patch.note_length_steps = Some(parse_u8(value, "instrument.note_length_steps")?);
                    }
                    InstrumentField::SendMfx => {
                        patch.send_mfx = Some(parse_u8(value, "instrument.send.mfx")?);
                    }
                    InstrumentField::SendDelay => {
                        patch.send_delay = Some(parse_u8(value, "instrument.send.delay")?);
                    }
                    InstrumentField::SendReverb => {
                        patch.send_reverb = Some(parse_u8(value, "instrument.send.reverb")?);
                    }
                    InstrumentField::SynthWaveform => {
                        patch.synth_waveform = Some(parse_waveform(value)?);
                    }
                    InstrumentField::SynthAttackMs => {
                        patch.synth_attack_ms = Some(parse_u16(value, "instrument.synth.attack_ms")?);
                    }
                    InstrumentField::SynthReleaseMs => {
                        patch.synth_release_ms = Some(parse_u16(value, "instrument.synth.release_ms")?);
                    }
                    InstrumentField::SynthGain => {
                        patch.synth_gain = Some(parse_u8(value, "instrument.synth.gain")?);
                    }
                    InstrumentField::SamplerVariant => {
                        patch.sampler_variant = Some(parse_sampler_variant(value)?);
                    }
                    InstrumentField::SamplerTransientLevel => {
                        patch.sampler_transient_level =
                            Some(parse_u8(value, "instrument.sampler.transient_level")?);
                    }
                    InstrumentField::SamplerBodyLevel => {
                        patch.sampler_body_level =
                            Some(parse_u8(value, "instrument.sampler.body_level")?);
                    }
                }
                continue;
            }

            if let Some((table_id, row, field)) = parse_table_field(key)? {
                let patch = table_row_patches.entry((table_id, row)).or_default();
                match field {
                    TableField::NoteOffset => {
                        patch.note_offset = Some(parse_i8(value, "table.row.note_offset")?)
                    }
                    TableField::Volume => patch.volume = Some(parse_u8(value, "table.row.volume")?),
                    TableField::Fx(slot) => {
                        if slot >= FX_SLOT_COUNT {
                            return Err(StorageError::InvalidIndex("fx_slot", slot));
                        }
                        patch
                            .fx
                            .insert(slot, Some(parse_fx_command(value, "table.row.fx")?));
                    }
                }
                continue;
            }

            if let Some(field) = parse_mixer_field(key)? {
                match field {
                    MixerField::TrackLevel(track_idx) => {
                        mixer_patch
                            .track_levels
                            .insert(track_idx, parse_u8(value, "mixer.track.level")?);
                    }
                    MixerField::MasterLevel => {
                        mixer_patch.master_level = Some(parse_u8(value, "mixer.master.level")?);
                    }
                    MixerField::SendMfx => {
                        mixer_patch.send_mfx = Some(parse_u8(value, "mixer.send.mfx")?);
                    }
                    MixerField::SendDelay => {
                        mixer_patch.send_delay = Some(parse_u8(value, "mixer.send.delay")?);
                    }
                    MixerField::SendReverb => {
                        mixer_patch.send_reverb = Some(parse_u8(value, "mixer.send.reverb")?);
                    }
                }
                continue;
            }

            if let Some(groove_id) = parse_groove_key(key)? {
                let ticks = if value.is_empty() {
                    Vec::new()
                } else {
                    value
                        .split(',')
                        .map(|s| parse_u8(s.trim(), "groove.values"))
                        .collect::<Result<Vec<_>, _>>()?
                };
                groove_map.insert(groove_id, ticks);
                continue;
            }

            if let Some((scale_id, field)) = parse_scale_field(key)? {
                let patch = scale_patches.entry(scale_id).or_default();
                match field {
                    ScaleField::Key => patch.key = Some(parse_u8(value, "scale.key")?),
                    ScaleField::Mask => patch.mask = Some(parse_u16(value, "scale.mask")?),
                }
                continue;
            }
        }

        let source_format_version =
            source_format_version.ok_or(StorageError::MissingField("format_version"))?;
        if source_format_version != FORMAT_VERSION && source_format_version != FORMAT_VERSION_V1 {
            return Err(StorageError::UnsupportedFormat(source_format_version));
        }

        let song_name = song_name.ok_or(StorageError::MissingField("song.name"))?;
        let tempo = tempo.ok_or(StorageError::MissingField("song.tempo"))?;
        if tempo == 0 {
            return Err(StorageError::ParseError("tempo must be > 0".to_string()));
        }

        let mut project = ProjectData::new(song_name);
        project.song.tempo = tempo;
        if let Some(id) = default_groove {
            project.song.default_groove = id;
        }
        if let Some(id) = default_scale {
            project.song.default_scale = id;
        }

        for (track_idx, mute) in track_mute {
            let track = project
                .song
                .tracks
                .get_mut(track_idx)
                .ok_or(StorageError::InvalidIndex("track", track_idx))?;
            track.mute = mute;
        }

        for (track_idx, solo) in track_solo {
            let track = project
                .song
                .tracks
                .get_mut(track_idx)
                .ok_or(StorageError::InvalidIndex("track", track_idx))?;
            track.solo = solo;
        }

        for (track_idx, override_id) in track_groove_override {
            let track = project
                .song
                .tracks
                .get_mut(track_idx)
                .ok_or(StorageError::InvalidIndex("track", track_idx))?;
            track.groove_override = override_id;
        }

        for (track_idx, override_id) in track_scale_override {
            let track = project
                .song
                .tracks
                .get_mut(track_idx)
                .ok_or(StorageError::InvalidIndex("track", track_idx))?;
            track.scale_override = override_id;
        }

        for ((track_idx, row), chain_id) in song_rows {
            if row >= SONG_ROW_COUNT {
                return Err(StorageError::InvalidIndex("song_row", row));
            }
            let track = project
                .song
                .tracks
                .get_mut(track_idx)
                .ok_or(StorageError::InvalidIndex("track", track_idx))?;
            track.song_rows[row] = Some(chain_id);
        }

        for ((chain_id, row), patch) in chain_patches {
            if row >= CHAIN_ROW_COUNT {
                return Err(StorageError::InvalidIndex("chain_row", row));
            }
            let chain = project
                .chains
                .entry(chain_id)
                .or_insert_with(|| Chain::new(chain_id));
            let chain_row = &mut chain.rows[row];

            if let Some(phrase_id) = patch.phrase_id {
                chain_row.phrase_id = phrase_id;
            }
            if let Some(transpose) = patch.transpose {
                chain_row.transpose = transpose;
            }
        }

        for ((phrase_id, step_idx), patch) in phrase_patches {
            if step_idx >= PHRASE_STEP_COUNT {
                return Err(StorageError::InvalidIndex("phrase_step", step_idx));
            }
            let phrase = project
                .phrases
                .entry(phrase_id)
                .or_insert_with(|| p9_core::model::Phrase::new(phrase_id));
            let step = &mut phrase.steps[step_idx];

            if let Some(note) = patch.note {
                step.note = note;
            }
            if let Some(velocity) = patch.velocity {
                step.velocity = velocity;
            }
            if let Some(instrument_id) = patch.instrument_id {
                step.instrument_id = instrument_id;
            }
            for (slot, command) in patch.fx {
                let target = step
                    .fx
                    .get_mut(slot)
                    .ok_or(StorageError::InvalidIndex("fx_slot", slot))?;
                *target = command;
            }
        }

        for (instrument_id, patch) in instrument_patches {
            let instrument = project
                .instruments
                .entry(instrument_id)
                .or_insert_with(|| {
                    Instrument::new(
                        instrument_id,
                        InstrumentType::None,
                        format!("Instrument {}", instrument_id),
                    )
                });

            if let Some(instrument_type) = patch.instrument_type {
                instrument.instrument_type = instrument_type;
            }
            if let Some(name) = patch.name {
                instrument.name = name;
            }
            if let Some(table_id) = patch.table_id {
                instrument.table_id = table_id;
            }
            if let Some(note_length_steps) = patch.note_length_steps {
                instrument.note_length_steps = note_length_steps.max(1);
            }
            if let Some(send_mfx) = patch.send_mfx {
                instrument.send_levels.mfx = send_mfx;
            }
            if let Some(send_delay) = patch.send_delay {
                instrument.send_levels.delay = send_delay;
            }
            if let Some(send_reverb) = patch.send_reverb {
                instrument.send_levels.reverb = send_reverb;
            }
            if let Some(waveform) = patch.synth_waveform {
                instrument.synth_params.waveform = waveform;
            }
            if let Some(attack_ms) = patch.synth_attack_ms {
                instrument.synth_params.attack_ms = attack_ms;
            }
            if let Some(release_ms) = patch.synth_release_ms {
                instrument.synth_params.release_ms = release_ms;
            }
            if let Some(gain) = patch.synth_gain {
                instrument.synth_params.gain = gain;
            }
            if patch.sampler_variant.is_some()
                || patch.sampler_transient_level.is_some()
                || patch.sampler_body_level.is_some()
            {
                let mut sampler_render = instrument.sampler_render.unwrap_or_default();
                if let Some(variant) = patch.sampler_variant {
                    sampler_render.variant = variant;
                }
                if let Some(level) = patch.sampler_transient_level {
                    sampler_render.transient_level = level;
                }
                if let Some(level) = patch.sampler_body_level {
                    sampler_render.body_level = level;
                }
                instrument.sampler_render = Some(sampler_render);
            }
        }

        for ((table_id, row_idx), patch) in table_row_patches {
            if row_idx >= CHAIN_ROW_COUNT {
                return Err(StorageError::InvalidIndex("table_row", row_idx));
            }
            let table = project
                .tables
                .entry(table_id)
                .or_insert_with(|| Table::new(table_id));
            let row = table
                .rows
                .get_mut(row_idx)
                .ok_or(StorageError::InvalidIndex("table_row", row_idx))?;

            if let Some(note_offset) = patch.note_offset {
                row.note_offset = note_offset;
            }
            if let Some(volume) = patch.volume {
                row.volume = volume;
            }
            for (slot, command) in patch.fx {
                let target = row
                    .fx
                    .get_mut(slot)
                    .ok_or(StorageError::InvalidIndex("fx_slot", slot))?;
                *target = command;
            }
        }

        for (groove_id, ticks_pattern) in groove_map {
            project.grooves.insert(
                groove_id,
                Groove {
                    id: groove_id,
                    ticks_pattern,
                },
            );
        }

        for (scale_id, patch) in scale_patches {
            let key = patch.key.ok_or(StorageError::MissingField("scale.key"))?;
            let mask = patch.mask.ok_or(StorageError::MissingField("scale.mask"))?;
            project.scales.insert(
                scale_id,
                Scale {
                    id: scale_id,
                    key,
                    interval_mask: mask,
                },
            );
        }

        for (track_idx, level) in mixer_patch.track_levels {
            if track_idx >= TRACK_COUNT {
                return Err(StorageError::InvalidIndex("mixer_track", track_idx));
            }
            project.mixer.track_levels[track_idx] = level;
        }

        if let Some(master_level) = mixer_patch.master_level {
            project.mixer.master_level = master_level;
        }
        if let Some(send_mfx) = mixer_patch.send_mfx {
            project.mixer.send_levels.mfx = send_mfx;
        }
        if let Some(send_delay) = mixer_patch.send_delay {
            project.mixer.send_levels.delay = send_delay;
        }
        if let Some(send_reverb) = mixer_patch.send_reverb {
            project.mixer.send_levels.reverb = send_reverb;
        }

        Ok(Self {
            format_version: FORMAT_VERSION,
            project,
        })
    }
}

fn render_opt_u8(value: Option<u8>) -> String {
    match value {
        Some(v) => v.to_string(),
        None => "none".to_string(),
    }
}

fn parse_u8(value: &str, field: &str) -> Result<u8, StorageError> {
    value
        .parse::<u8>()
        .map_err(|_| StorageError::ParseError(field.to_string()))
}

fn parse_u16(value: &str, field: &str) -> Result<u16, StorageError> {
    value
        .parse::<u16>()
        .map_err(|_| StorageError::ParseError(field.to_string()))
}

fn parse_i8(value: &str, field: &str) -> Result<i8, StorageError> {
    value
        .parse::<i8>()
        .map_err(|_| StorageError::ParseError(field.to_string()))
}

fn parse_bool(value: &str, field: &str) -> Result<bool, StorageError> {
    match value {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        _ => Err(StorageError::ParseError(field.to_string())),
    }
}

fn parse_opt_u8(value: &str, field: &str) -> Result<Option<u8>, StorageError> {
    if value.eq_ignore_ascii_case("none") {
        Ok(None)
    } else {
        parse_u8(value, field).map(Some)
    }
}

fn render_fx_command(command: &FxCommand) -> String {
    format!("{}:{}", command.code, command.value)
}

fn parse_fx_command(value: &str, field: &str) -> Result<FxCommand, StorageError> {
    let Some((code_raw, value_raw)) = value.split_once(':') else {
        return Err(StorageError::ParseError(field.to_string()));
    };
    let code = code_raw.trim().to_ascii_uppercase();
    if code.is_empty() {
        return Err(StorageError::ParseError(field.to_string()));
    }
    let fx_value = parse_u8(value_raw.trim(), field)?;

    Ok(FxCommand {
        code,
        value: fx_value,
    })
}

fn render_instrument_type(instrument_type: &InstrumentType) -> &'static str {
    match instrument_type {
        InstrumentType::None => "none",
        InstrumentType::Synth => "synth",
        InstrumentType::Sampler => "sampler",
        InstrumentType::MidiOut => "midi_out",
        InstrumentType::External => "external",
    }
}

fn parse_instrument_type(value: &str) -> Result<InstrumentType, StorageError> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Ok(InstrumentType::None),
        "synth" => Ok(InstrumentType::Synth),
        "sampler" => Ok(InstrumentType::Sampler),
        "midi_out" | "midiout" => Ok(InstrumentType::MidiOut),
        "external" => Ok(InstrumentType::External),
        _ => Err(StorageError::ParseError("instrument.type".to_string())),
    }
}

fn render_waveform(waveform: SynthWaveform) -> &'static str {
    match waveform {
        SynthWaveform::Sine => "sine",
        SynthWaveform::Square => "square",
        SynthWaveform::Saw => "saw",
        SynthWaveform::Triangle => "triangle",
    }
}

fn parse_waveform(value: &str) -> Result<SynthWaveform, StorageError> {
    match value.to_ascii_lowercase().as_str() {
        "sine" => Ok(SynthWaveform::Sine),
        "square" => Ok(SynthWaveform::Square),
        "saw" => Ok(SynthWaveform::Saw),
        "triangle" => Ok(SynthWaveform::Triangle),
        _ => Err(StorageError::ParseError("instrument.synth.waveform".to_string())),
    }
}

fn render_sampler_variant(variant: SamplerRenderVariant) -> &'static str {
    match variant {
        SamplerRenderVariant::Classic => "classic",
        SamplerRenderVariant::Punch => "punch",
        SamplerRenderVariant::Air => "air",
    }
}

fn parse_sampler_variant(value: &str) -> Result<SamplerRenderVariant, StorageError> {
    match value.to_ascii_lowercase().as_str() {
        "classic" => Ok(SamplerRenderVariant::Classic),
        "punch" => Ok(SamplerRenderVariant::Punch),
        "air" => Ok(SamplerRenderVariant::Air),
        _ => Err(StorageError::ParseError(
            "instrument.sampler.variant".to_string(),
        )),
    }
}

enum TrackField {
    Mute,
    Solo,
    GrooveOverride,
    ScaleOverride,
    SongRowChain(usize),
}

fn parse_track_field(key: &str) -> Result<Option<(usize, TrackField)>, StorageError> {
    if !key.starts_with("track.") {
        return Ok(None);
    }

    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() < 3 {
        return Ok(None);
    }

    let track_idx = parts[1]
        .parse::<usize>()
        .map_err(|_| StorageError::ParseError("track.index".to_string()))?;

    if track_idx >= TRACK_COUNT {
        return Err(StorageError::InvalidIndex("track", track_idx));
    }

    if parts.len() == 3 {
        return match parts[2] {
            "mute" => Ok(Some((track_idx, TrackField::Mute))),
            "solo" => Ok(Some((track_idx, TrackField::Solo))),
            "groove_override" => Ok(Some((track_idx, TrackField::GrooveOverride))),
            "scale_override" => Ok(Some((track_idx, TrackField::ScaleOverride))),
            _ => Ok(None),
        };
    }

    if parts.len() == 5 && parts[2] == "row" && parts[4] == "chain" {
        let row = parts[3]
            .parse::<usize>()
            .map_err(|_| StorageError::ParseError("track.row".to_string()))?;
        if row >= SONG_ROW_COUNT {
            return Err(StorageError::InvalidIndex("song_row", row));
        }
        return Ok(Some((track_idx, TrackField::SongRowChain(row))));
    }

    Ok(None)
}

enum ChainField {
    Phrase,
    Transpose,
}

fn parse_chain_field(key: &str) -> Result<Option<(u8, usize, ChainField)>, StorageError> {
    if !key.starts_with("chain.") {
        return Ok(None);
    }

    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() != 5 || parts[2] != "row" {
        return Ok(None);
    }

    let chain_id = parse_u8(parts[1], "chain.id")?;
    let row = parts[3]
        .parse::<usize>()
        .map_err(|_| StorageError::ParseError("chain.row".to_string()))?;

    if row >= CHAIN_ROW_COUNT {
        return Err(StorageError::InvalidIndex("chain_row", row));
    }

    let field = match parts[4] {
        "phrase" => ChainField::Phrase,
        "transpose" => ChainField::Transpose,
        _ => return Ok(None),
    };

    Ok(Some((chain_id, row, field)))
}

enum PhraseField {
    Note,
    Velocity,
    Instrument,
    Fx(usize),
}

fn parse_phrase_field(key: &str) -> Result<Option<(u8, usize, PhraseField)>, StorageError> {
    if !key.starts_with("phrase.") {
        return Ok(None);
    }

    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() < 5 || parts[2] != "step" {
        return Ok(None);
    }

    let phrase_id = parse_u8(parts[1], "phrase.id")?;
    let step = parts[3]
        .parse::<usize>()
        .map_err(|_| StorageError::ParseError("phrase.step".to_string()))?;

    if step >= PHRASE_STEP_COUNT {
        return Err(StorageError::InvalidIndex("phrase_step", step));
    }

    if parts.len() == 5 {
        let field = match parts[4] {
            "note" => PhraseField::Note,
            "velocity" => PhraseField::Velocity,
            "instrument" => PhraseField::Instrument,
            _ => return Ok(None),
        };
        return Ok(Some((phrase_id, step, field)));
    }

    if parts.len() == 7 && parts[4] == "fx" {
        let slot = parts[5]
            .parse::<usize>()
            .map_err(|_| StorageError::ParseError("phrase.step.fx.slot".to_string()))?;
        let field_name = parts[6];
        if field_name != "value" {
            // Accepted compact form: phrase.{id}.step.{step}.fx.{slot}=CODE:VAL
            if field_name.is_empty() {
                return Ok(None);
            }
        }
        return Ok(Some((phrase_id, step, PhraseField::Fx(slot))));
    }

    if parts.len() == 6 && parts[4] == "fx" {
        let slot = parts[5]
            .parse::<usize>()
            .map_err(|_| StorageError::ParseError("phrase.step.fx.slot".to_string()))?;
        return Ok(Some((phrase_id, step, PhraseField::Fx(slot))));
    }

    Ok(None)
}

enum InstrumentField {
    Type,
    Name,
    Table,
    NoteLengthSteps,
    SendMfx,
    SendDelay,
    SendReverb,
    SynthWaveform,
    SynthAttackMs,
    SynthReleaseMs,
    SynthGain,
    SamplerVariant,
    SamplerTransientLevel,
    SamplerBodyLevel,
}

fn parse_instrument_field(key: &str) -> Result<Option<(u8, InstrumentField)>, StorageError> {
    if !key.starts_with("instrument.") {
        return Ok(None);
    }

    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() < 3 {
        return Ok(None);
    }

    let instrument_id = parse_u8(parts[1], "instrument.id")?;

    if parts.len() == 3 {
        let field = match parts[2] {
            "type" => InstrumentField::Type,
            "name" => InstrumentField::Name,
            "table" => InstrumentField::Table,
            "note_length_steps" => InstrumentField::NoteLengthSteps,
            _ => return Ok(None),
        };
        return Ok(Some((instrument_id, field)));
    }

    if parts.len() == 4 && parts[2] == "send" {
        let field = match parts[3] {
            "mfx" => InstrumentField::SendMfx,
            "delay" => InstrumentField::SendDelay,
            "reverb" => InstrumentField::SendReverb,
            _ => return Ok(None),
        };
        return Ok(Some((instrument_id, field)));
    }

    if parts.len() == 4 && parts[2] == "synth" {
        let field = match parts[3] {
            "waveform" => InstrumentField::SynthWaveform,
            "attack_ms" => InstrumentField::SynthAttackMs,
            "release_ms" => InstrumentField::SynthReleaseMs,
            "gain" => InstrumentField::SynthGain,
            _ => return Ok(None),
        };
        return Ok(Some((instrument_id, field)));
    }

    if parts.len() == 4 && parts[2] == "sampler" {
        let field = match parts[3] {
            "variant" => InstrumentField::SamplerVariant,
            "transient_level" => InstrumentField::SamplerTransientLevel,
            "body_level" => InstrumentField::SamplerBodyLevel,
            _ => return Ok(None),
        };
        return Ok(Some((instrument_id, field)));
    }

    Ok(None)
}

enum TableField {
    NoteOffset,
    Volume,
    Fx(usize),
}

fn parse_table_field(key: &str) -> Result<Option<(u8, usize, TableField)>, StorageError> {
    if !key.starts_with("table.") {
        return Ok(None);
    }

    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() < 5 || parts[2] != "row" {
        return Ok(None);
    }

    let table_id = parse_u8(parts[1], "table.id")?;
    let row = parts[3]
        .parse::<usize>()
        .map_err(|_| StorageError::ParseError("table.row".to_string()))?;

    if row >= CHAIN_ROW_COUNT {
        return Err(StorageError::InvalidIndex("table_row", row));
    }

    if parts.len() == 5 {
        let field = match parts[4] {
            "note_offset" => TableField::NoteOffset,
            "volume" => TableField::Volume,
            _ => return Ok(None),
        };
        return Ok(Some((table_id, row, field)));
    }

    if parts.len() == 7 && parts[4] == "fx" {
        let slot = parts[5]
            .parse::<usize>()
            .map_err(|_| StorageError::ParseError("table.row.fx.slot".to_string()))?;
        if parts[6] != "value" {
            // Accepted compact form: table.{id}.row.{row}.fx.{slot}=CODE:VAL
            if parts[6].is_empty() {
                return Ok(None);
            }
        }
        return Ok(Some((table_id, row, TableField::Fx(slot))));
    }

    if parts.len() == 6 && parts[4] == "fx" {
        let slot = parts[5]
            .parse::<usize>()
            .map_err(|_| StorageError::ParseError("table.row.fx.slot".to_string()))?;
        return Ok(Some((table_id, row, TableField::Fx(slot))));
    }

    Ok(None)
}

enum MixerField {
    TrackLevel(usize),
    MasterLevel,
    SendMfx,
    SendDelay,
    SendReverb,
}

fn parse_mixer_field(key: &str) -> Result<Option<MixerField>, StorageError> {
    if !key.starts_with("mixer.") {
        return Ok(None);
    }

    let parts: Vec<&str> = key.split('.').collect();

    if parts.len() == 4 && parts[1] == "track" && parts[3] == "level" {
        let track_idx = parts[2]
            .parse::<usize>()
            .map_err(|_| StorageError::ParseError("mixer.track.index".to_string()))?;
        if track_idx >= TRACK_COUNT {
            return Err(StorageError::InvalidIndex("mixer_track", track_idx));
        }
        return Ok(Some(MixerField::TrackLevel(track_idx)));
    }

    if parts.len() == 3 && parts[1] == "master" && parts[2] == "level" {
        return Ok(Some(MixerField::MasterLevel));
    }

    if parts.len() == 3 && parts[1] == "send" {
        let field = match parts[2] {
            "mfx" => MixerField::SendMfx,
            "delay" => MixerField::SendDelay,
            "reverb" => MixerField::SendReverb,
            _ => return Ok(None),
        };
        return Ok(Some(field));
    }

    Ok(None)
}

fn parse_groove_key(key: &str) -> Result<Option<u8>, StorageError> {
    if !key.starts_with("groove.") {
        return Ok(None);
    }

    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() != 2 {
        return Ok(None);
    }

    let groove_id = parse_u8(parts[1], "groove.id")?;
    Ok(Some(groove_id))
}

enum ScaleField {
    Key,
    Mask,
}

fn parse_scale_field(key: &str) -> Result<Option<(u8, ScaleField)>, StorageError> {
    if !key.starts_with("scale.") {
        return Ok(None);
    }

    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() != 3 {
        return Ok(None);
    }

    let scale_id = parse_u8(parts[1], "scale.id")?;
    let field = match parts[2] {
        "key" => ScaleField::Key,
        "mask" => ScaleField::Mask,
        _ => return Ok(None),
    };

    Ok(Some((scale_id, field)))
}

#[cfg(test)]
mod tests {
    use super::{ProjectEnvelope, StorageError, FORMAT_VERSION};
    use p9_core::model::{
        Chain, FxCommand, Groove, Instrument, InstrumentType, ProjectData, SamplerRenderParams,
        SamplerRenderVariant, Scale, SynthWaveform, Table,
    };

    #[test]
    fn round_trip_text_preserves_song_basics() {
        let mut project = ProjectData::new("unit");
        project.song.tempo = 133;
        let envelope = ProjectEnvelope::new(project);

        let text = envelope.to_text();
        let restored = ProjectEnvelope::from_text(&text).unwrap();

        assert_eq!(restored.format_version, FORMAT_VERSION);
        assert_eq!(restored.project.song.name, "unit");
        assert_eq!(restored.project.song.tempo, 133);
    }

    #[test]
    fn round_trip_preserves_arrangement_and_overrides() {
        let mut project = ProjectData::new("arr");
        project.song.tempo = 140;
        project.song.default_groove = 1;
        project.song.default_scale = 2;

        project.song.tracks[0].mute = true;
        project.song.tracks[0].solo = true;
        project.song.tracks[0].groove_override = Some(3);
        project.song.tracks[0].scale_override = Some(4);
        project.song.tracks[0].song_rows[0] = Some(7);

        let mut chain = Chain::new(7);
        chain.rows[0].phrase_id = Some(2);
        chain.rows[0].transpose = 1;
        project.chains.insert(7, chain);

        let phrase = project
            .phrases
            .entry(2)
            .or_insert_with(|| p9_core::model::Phrase::new(2));
        phrase.steps[0].note = Some(61);
        phrase.steps[0].velocity = 95;
        phrase.steps[0].instrument_id = Some(5);

        project.grooves.insert(
            1,
            Groove {
                id: 1,
                ticks_pattern: vec![6, 6, 3, 9],
            },
        );
        project.grooves.insert(
            3,
            Groove {
                id: 3,
                ticks_pattern: vec![2, 2, 2, 2],
            },
        );

        project.scales.insert(
            2,
            Scale {
                id: 2,
                key: 0,
                interval_mask: major_scale_mask(),
            },
        );
        project.scales.insert(
            4,
            Scale {
                id: 4,
                key: 2,
                interval_mask: major_scale_mask(),
            },
        );

        let envelope = ProjectEnvelope::new(project);
        let text = envelope.to_text();
        let restored = ProjectEnvelope::from_text(&text).unwrap();

        assert_eq!(restored.project.song.name, "arr");
        assert_eq!(restored.project.song.tempo, 140);
        assert_eq!(restored.project.song.default_groove, 1);
        assert_eq!(restored.project.song.default_scale, 2);

        let track0 = &restored.project.song.tracks[0];
        assert!(track0.mute);
        assert!(track0.solo);
        assert_eq!(track0.groove_override, Some(3));
        assert_eq!(track0.scale_override, Some(4));
        assert_eq!(track0.song_rows[0], Some(7));

        let restored_chain = restored.project.chains.get(&7).unwrap();
        assert_eq!(restored_chain.rows[0].phrase_id, Some(2));
        assert_eq!(restored_chain.rows[0].transpose, 1);

        let restored_phrase = restored.project.phrases.get(&2).unwrap();
        assert_eq!(restored_phrase.steps[0].note, Some(61));
        assert_eq!(restored_phrase.steps[0].velocity, 95);
        assert_eq!(restored_phrase.steps[0].instrument_id, Some(5));

        assert_eq!(
            restored.project.grooves.get(&1).unwrap().ticks_pattern,
            vec![6, 6, 3, 9]
        );
        assert_eq!(
            restored.project.grooves.get(&3).unwrap().ticks_pattern,
            vec![2, 2, 2, 2]
        );
        assert_eq!(restored.project.scales.get(&2).unwrap().key, 0);
        assert_eq!(restored.project.scales.get(&4).unwrap().key, 2);
    }

    #[test]
    fn round_trip_preserves_instruments_tables_mixer_and_fx() {
        let mut project = ProjectData::new("v2");

        let mut instrument = Instrument::new(3, InstrumentType::Synth, "Lead");
        instrument.table_id = Some(7);
        instrument.note_length_steps = 4;
        instrument.send_levels.mfx = 11;
        instrument.send_levels.delay = 22;
        instrument.send_levels.reverb = 33;
        instrument.synth_params.waveform = SynthWaveform::Square;
        instrument.synth_params.attack_ms = 12;
        instrument.synth_params.release_ms = 240;
        instrument.synth_params.gain = 87;
        project.instruments.insert(3, instrument);

        let mut sampler = Instrument::new(4, InstrumentType::Sampler, "Drum");
        sampler.sampler_render = Some(SamplerRenderParams {
            variant: SamplerRenderVariant::Punch,
            transient_level: 112,
            body_level: 44,
        });
        project.instruments.insert(4, sampler);

        let mut table = Table::new(7);
        table.rows[0].note_offset = 3;
        table.rows[0].volume = 90;
        table.rows[0].fx[0] = Some(FxCommand {
            code: "TRN".to_string(),
            value: 50,
        });
        project.tables.insert(7, table);

        let phrase = project
            .phrases
            .entry(5)
            .or_insert_with(|| p9_core::model::Phrase::new(5));
        phrase.steps[0].note = Some(64);
        phrase.steps[0].velocity = 100;
        phrase.steps[0].instrument_id = Some(3);
        phrase.steps[0].fx[0] = Some(FxCommand {
            code: "VOL".to_string(),
            value: 80,
        });

        project.mixer.track_levels[0] = 98;
        project.mixer.master_level = 115;
        project.mixer.send_levels.mfx = 4;
        project.mixer.send_levels.delay = 5;
        project.mixer.send_levels.reverb = 6;

        let text = ProjectEnvelope::new(project).to_text();
        let restored = ProjectEnvelope::from_text(&text).unwrap();

        let instrument = restored.project.instruments.get(&3).unwrap();
        assert_eq!(instrument.name, "Lead");
        assert_eq!(instrument.table_id, Some(7));
        assert_eq!(instrument.note_length_steps, 4);
        assert_eq!(instrument.send_levels.mfx, 11);
        assert_eq!(instrument.send_levels.delay, 22);
        assert_eq!(instrument.send_levels.reverb, 33);
        assert_eq!(instrument.synth_params.waveform, SynthWaveform::Square);
        assert_eq!(instrument.synth_params.attack_ms, 12);
        assert_eq!(instrument.synth_params.release_ms, 240);
        assert_eq!(instrument.synth_params.gain, 87);

        let sampler = restored.project.instruments.get(&4).unwrap();
        assert_eq!(sampler.instrument_type, InstrumentType::Sampler);
        let sampler_render = sampler.sampler_render.expect("sampler render params");
        assert_eq!(sampler_render.variant, SamplerRenderVariant::Punch);
        assert_eq!(sampler_render.transient_level, 112);
        assert_eq!(sampler_render.body_level, 44);

        let table = restored.project.tables.get(&7).unwrap();
        assert_eq!(table.rows[0].note_offset, 3);
        assert_eq!(table.rows[0].volume, 90);
        assert_eq!(table.rows[0].fx[0].as_ref().unwrap().code, "TRN");

        let phrase = restored.project.phrases.get(&5).unwrap();
        assert_eq!(phrase.steps[0].fx[0].as_ref().unwrap().code, "VOL");
        assert_eq!(phrase.steps[0].fx[0].as_ref().unwrap().value, 80);

        assert_eq!(restored.project.mixer.track_levels[0], 98);
        assert_eq!(restored.project.mixer.master_level, 115);
        assert_eq!(restored.project.mixer.send_levels.mfx, 4);
        assert_eq!(restored.project.mixer.send_levels.delay, 5);
        assert_eq!(restored.project.mixer.send_levels.reverb, 6);
    }

    #[test]
    fn from_text_migrates_v1_to_v2() {
        let input = "format_version=1\nname=legacy\ntempo=111\n";
        let restored = ProjectEnvelope::from_text(input).unwrap();

        assert_eq!(restored.format_version, FORMAT_VERSION);
        assert_eq!(restored.project.song.name, "legacy");
        assert_eq!(restored.project.song.tempo, 111);
    }

    #[test]
    fn from_text_rejects_out_of_range_track_index() {
        let input = format!(
            "format_version={}\nsong.name=bad\nsong.tempo=120\ntrack.99.mute=1\n",
            FORMAT_VERSION
        );
        let error = ProjectEnvelope::from_text(&input).unwrap_err();

        match error {
            StorageError::InvalidIndex("track", 99) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    fn major_scale_mask() -> u16 {
        let intervals = [0u16, 2, 4, 5, 7, 9, 11];
        let mut mask = 0u16;
        for i in intervals {
            mask |= 1 << i;
        }
        mask
    }
}
