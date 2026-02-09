use std::collections::HashMap;

use p9_core::model::{
    Chain, Groove, ProjectData, Scale, CHAIN_ROW_COUNT, PHRASE_STEP_COUNT, SONG_ROW_COUNT,
    TRACK_COUNT,
};

pub const FORMAT_VERSION: u16 = 1;

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
}

#[derive(Clone, Debug, Default)]
struct ScalePatch {
    key: Option<u8>,
    mask: Option<u16>,
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

        lines.push(format!("format_version={}", self.format_version));
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
                    if chain_row.phrase_id.is_some() || chain_row.transpose != 0 {
                        lines.push(format!(
                            "chain.{}.row.{}.phrase={}",
                            chain_id,
                            row,
                            render_opt_u8(chain_row.phrase_id)
                        ));
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
                    let is_default =
                        step.note.is_none() && step.velocity == 0x40 && step.instrument_id.is_none();

                    if !is_default {
                        lines.push(format!(
                            "phrase.{}.step.{}.note={}",
                            phrase_id,
                            step_idx,
                            render_opt_u8(step.note)
                        ));
                        lines.push(format!(
                            "phrase.{}.step.{}.velocity={}",
                            phrase_id, step_idx, step.velocity
                        ));
                        lines.push(format!(
                            "phrase.{}.step.{}.instrument={}",
                            phrase_id,
                            step_idx,
                            render_opt_u8(step.instrument_id)
                        ));
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

        lines.join("\n") + "\n"
    }

    pub fn from_text(input: &str) -> Result<Self, StorageError> {
        let mut format_version = None;
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
                    format_version = Some(parse_u16(value, "format_version")?);
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
                        track_groove_override.insert(track_idx, parse_opt_u8(value, "track.groove_override")?);
                    }
                    TrackField::ScaleOverride => {
                        track_scale_override.insert(track_idx, parse_opt_u8(value, "track.scale_override")?);
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
                    ChainField::Phrase => patch.phrase_id = Some(parse_opt_u8(value, "chain.row.phrase")?),
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
                    PhraseField::Velocity => patch.velocity = Some(parse_u8(value, "phrase.step.velocity")?),
                    PhraseField::Instrument => {
                        patch.instrument_id = Some(parse_opt_u8(value, "phrase.step.instrument")?)
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

        let format_version = format_version.ok_or(StorageError::MissingField("format_version"))?;
        if format_version != FORMAT_VERSION {
            return Err(StorageError::UnsupportedFormat(format_version));
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

        Ok(Self {
            format_version,
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
}

fn parse_phrase_field(key: &str) -> Result<Option<(u8, usize, PhraseField)>, StorageError> {
    if !key.starts_with("phrase.") {
        return Ok(None);
    }

    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() != 5 || parts[2] != "step" {
        return Ok(None);
    }

    let phrase_id = parse_u8(parts[1], "phrase.id")?;
    let step = parts[3]
        .parse::<usize>()
        .map_err(|_| StorageError::ParseError("phrase.step".to_string()))?;

    if step >= PHRASE_STEP_COUNT {
        return Err(StorageError::InvalidIndex("phrase_step", step));
    }

    let field = match parts[4] {
        "note" => PhraseField::Note,
        "velocity" => PhraseField::Velocity,
        "instrument" => PhraseField::Instrument,
        _ => return Ok(None),
    };

    Ok(Some((phrase_id, step, field)))
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
    use p9_core::model::{Chain, Groove, ProjectData, Scale};

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

        assert_eq!(restored.project.grooves.get(&1).unwrap().ticks_pattern, vec![6, 6, 3, 9]);
        assert_eq!(restored.project.grooves.get(&3).unwrap().ticks_pattern, vec![2, 2, 2, 2]);
        assert_eq!(restored.project.scales.get(&2).unwrap().key, 0);
        assert_eq!(restored.project.scales.get(&4).unwrap().key, 2);
    }

    #[test]
    fn from_text_accepts_legacy_song_keys() {
        let input = format!("format_version={}\nname=legacy\ntempo=111\n", FORMAT_VERSION);
        let restored = ProjectEnvelope::from_text(&input).unwrap();

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
