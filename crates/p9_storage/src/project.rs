use p9_core::model::ProjectData;

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
        format!(
            "format_version={}\nname={}\ntempo={}\n",
            self.format_version, self.project.song.name, self.project.song.tempo
        )
    }

    pub fn from_text(input: &str) -> Result<Self, StorageError> {
        let mut format_version = None;
        let mut song_name = None;
        let mut tempo = None;

        for line in input.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            match key.trim() {
                "format_version" => {
                    let parsed = value
                        .trim()
                        .parse::<u16>()
                        .map_err(|_| StorageError::ParseError("format_version".to_string()))?;
                    format_version = Some(parsed);
                }
                "name" => {
                    song_name = Some(value.trim().to_string());
                }
                "tempo" => {
                    let parsed = value
                        .trim()
                        .parse::<u16>()
                        .map_err(|_| StorageError::ParseError("tempo".to_string()))?;
                    tempo = Some(parsed);
                }
                _ => {}
            }
        }

        let format_version =
            format_version.ok_or(StorageError::MissingField("format_version"))?;
        if format_version != FORMAT_VERSION {
            return Err(StorageError::UnsupportedFormat(format_version));
        }

        let song_name = song_name.ok_or(StorageError::MissingField("name"))?;
        let tempo = tempo.ok_or(StorageError::MissingField("tempo"))?;

        if tempo == 0 {
            return Err(StorageError::ParseError("tempo must be > 0".to_string()));
        }

        let mut project = ProjectData::new(song_name);
        project.song.tempo = tempo;

        Ok(Self {
            format_version,
            project,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ProjectEnvelope, FORMAT_VERSION};
    use p9_core::model::ProjectData;

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
}
