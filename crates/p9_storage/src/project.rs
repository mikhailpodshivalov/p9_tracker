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
}
