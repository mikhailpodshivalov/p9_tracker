use std::fs;
use std::io;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;

use p9_core::engine::Engine;
use p9_storage::project::ProjectEnvelope;

use crate::runtime::TransportSnapshot;

const AUTOSAVE_FILE_NAME: &str = "p9_tracker_phase16_autosave.p9";
const DIRTY_FLAG_FILE_NAME: &str = "p9_tracker_phase16_dirty.flag";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutosavePolicy {
    pub interval_ticks: u64,
}

impl Default for AutosavePolicy {
    fn default() -> Self {
        Self { interval_ticks: 96 }
    }
}

#[derive(Clone, Debug)]
pub struct AutosaveManager {
    policy: AutosavePolicy,
    last_saved_tick: u64,
}

#[derive(Debug)]
pub enum AutosaveError {
    Io(io::Error),
}

impl From<io::Error> for AutosaveError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryStatus {
    CleanStart,
    RecoveredFromAutosave,
    DirtyFlagWithoutAutosave,
    AutosaveReadFailed,
    AutosaveParseFailed,
}

impl RecoveryStatus {
    pub fn label(self) -> &'static str {
        match self {
            RecoveryStatus::CleanStart => "clean-start",
            RecoveryStatus::RecoveredFromAutosave => "recovered",
            RecoveryStatus::DirtyFlagWithoutAutosave => "dirty-flag-without-autosave",
            RecoveryStatus::AutosaveReadFailed => "autosave-read-failed",
            RecoveryStatus::AutosaveParseFailed => "autosave-parse-failed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DirtyStateTracker {
    saved_fingerprint: String,
}

impl DirtyStateTracker {
    pub fn from_engine(engine: &Engine) -> Self {
        Self {
            saved_fingerprint: project_fingerprint(engine),
        }
    }

    pub fn is_dirty(&self, engine: &Engine) -> bool {
        self.saved_fingerprint != project_fingerprint(engine)
    }

    pub fn mark_saved(&mut self, engine: &Engine) {
        self.saved_fingerprint = project_fingerprint(engine);
    }
}

pub fn default_autosave_path() -> PathBuf {
    std::env::temp_dir().join(AUTOSAVE_FILE_NAME)
}

pub fn default_dirty_flag_path() -> PathBuf {
    std::env::temp_dir().join(DIRTY_FLAG_FILE_NAME)
}

pub fn mark_dirty_session_flag(path: impl AsRef<Path>) -> Result<(), AutosaveError> {
    fs::write(path, "dirty=1\n")?;
    Ok(())
}

pub fn clear_dirty_session_flag(path: impl AsRef<Path>) -> Result<(), AutosaveError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

pub fn recover_from_dirty_session(
    engine: &mut Engine,
    autosave_path: impl AsRef<Path>,
    dirty_flag_path: impl AsRef<Path>,
) -> RecoveryStatus {
    let dirty_flag_path = dirty_flag_path.as_ref();
    if !dirty_flag_path.exists() {
        return RecoveryStatus::CleanStart;
    }

    let autosave_path = autosave_path.as_ref();
    let autosave_text = match fs::read_to_string(autosave_path) {
        Ok(text) => text,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            let _ = clear_dirty_session_flag(dirty_flag_path);
            return RecoveryStatus::DirtyFlagWithoutAutosave;
        }
        Err(_) => return RecoveryStatus::AutosaveReadFailed,
    };

    let envelope = match ProjectEnvelope::from_text(&autosave_text) {
        Ok(envelope) => envelope,
        Err(_) => return RecoveryStatus::AutosaveParseFailed,
    };

    engine.replace_project(envelope.project);
    let _ = clear_dirty_session_flag(dirty_flag_path);
    RecoveryStatus::RecoveredFromAutosave
}

impl AutosaveManager {
    pub fn new(policy: AutosavePolicy) -> Self {
        Self {
            policy,
            last_saved_tick: 0,
        }
    }

    pub fn save_if_due(
        &mut self,
        engine: &Engine,
        transport: TransportSnapshot,
        dirty: bool,
        path: impl AsRef<Path>,
    ) -> Result<bool, AutosaveError> {
        if !dirty {
            return Ok(false);
        }

        let due_tick = self
            .last_saved_tick
            .saturating_add(self.policy.interval_ticks.max(1));
        if transport.tick < due_tick {
            return Ok(false);
        }

        let envelope = ProjectEnvelope::new(engine.snapshot().clone());
        fs::write(path, envelope.to_text())?;
        self.last_saved_tick = transport.tick;
        Ok(true)
    }

    pub fn last_saved_tick(&self) -> u64 {
        self.last_saved_tick
    }
}

fn project_fingerprint(engine: &Engine) -> String {
    ProjectEnvelope::new(engine.snapshot().clone()).to_text()
}

#[cfg(test)]
mod tests {
    use super::{
        clear_dirty_session_flag, default_autosave_path, default_dirty_flag_path,
        mark_dirty_session_flag, recover_from_dirty_session, AutosaveManager, AutosavePolicy,
        DirtyStateTracker, RecoveryStatus,
    };
    use crate::runtime::{SyncMode, TransportSnapshot};
    use p9_core::engine::{Engine, EngineCommand};
    use p9_storage::project::ProjectEnvelope;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn snapshot(tick: u64) -> TransportSnapshot {
        TransportSnapshot {
            tick,
            is_playing: true,
            sync_mode: SyncMode::Internal,
            external_clock_pending: 0,
            queued_commands: 0,
            processed_commands: 0,
            midi_messages_ingested_total: 0,
        }
    }

    fn temp_file(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let mut path = std::env::temp_dir();
        path.push(format!("{}_{}_{}.p9", prefix, std::process::id(), nanos));
        path
    }

    #[test]
    fn save_if_due_writes_snapshot_when_dirty() {
        let engine = Engine::new("autosave");
        let mut autosave = AutosaveManager::new(AutosavePolicy { interval_ticks: 8 });
        let path = temp_file("p9_autosave_due");

        let saved = autosave
            .save_if_due(&engine, snapshot(8), true, &path)
            .unwrap();

        assert!(saved);
        assert_eq!(autosave.last_saved_tick(), 8);
        assert!(fs::metadata(&path).is_ok());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn save_if_due_skips_when_not_due_or_not_dirty() {
        let engine = Engine::new("autosave");
        let mut autosave = AutosaveManager::new(AutosavePolicy { interval_ticks: 16 });
        let path = temp_file("p9_autosave_skip");

        let first = autosave
            .save_if_due(&engine, snapshot(8), true, &path)
            .unwrap();
        assert!(!first);

        let second = autosave
            .save_if_due(&engine, snapshot(16), false, &path)
            .unwrap();
        assert!(!second);
        assert!(fs::metadata(&path).is_err());
    }

    #[test]
    fn dirty_state_tracker_marks_dirty_and_resets_on_save() {
        let mut engine = Engine::new("dirty");
        let mut tracker = DirtyStateTracker::from_engine(&engine);

        assert!(!tracker.is_dirty(&engine));

        engine.apply_command(EngineCommand::SetTempo(156)).unwrap();
        assert!(tracker.is_dirty(&engine));

        tracker.mark_saved(&engine);
        assert!(!tracker.is_dirty(&engine));
    }

    #[test]
    fn recover_from_dirty_session_restores_project_snapshot() {
        let autosave_path = temp_file("p9_recover_snapshot");
        let dirty_flag_path = temp_file("p9_recover_flag");

        let mut saved_engine = Engine::new("recovery");
        saved_engine
            .apply_command(EngineCommand::SetTempo(188))
            .unwrap();
        let envelope = ProjectEnvelope::new(saved_engine.snapshot().clone());
        fs::write(&autosave_path, envelope.to_text()).unwrap();
        mark_dirty_session_flag(&dirty_flag_path).unwrap();

        let mut restored = Engine::new("fresh");
        let status = recover_from_dirty_session(&mut restored, &autosave_path, &dirty_flag_path);

        assert_eq!(status, RecoveryStatus::RecoveredFromAutosave);
        assert_eq!(restored.snapshot().song.tempo, 188);
        assert!(fs::metadata(&dirty_flag_path).is_err());

        let _ = fs::remove_file(&autosave_path);
        let _ = fs::remove_file(&dirty_flag_path);
    }

    #[test]
    fn recover_from_dirty_session_handles_missing_snapshot() {
        let autosave_path = temp_file("p9_recover_missing_snapshot");
        let dirty_flag_path = temp_file("p9_recover_missing_flag");

        mark_dirty_session_flag(&dirty_flag_path).unwrap();
        let mut engine = Engine::new("missing");
        let status = recover_from_dirty_session(&mut engine, &autosave_path, &dirty_flag_path);

        assert_eq!(status, RecoveryStatus::DirtyFlagWithoutAutosave);
        assert!(fs::metadata(&dirty_flag_path).is_err());
    }

    #[test]
    fn recover_from_dirty_session_reports_parse_failure() {
        let autosave_path = temp_file("p9_recover_bad_snapshot");
        let dirty_flag_path = temp_file("p9_recover_bad_flag");

        fs::write(&autosave_path, "not-a-valid-envelope").unwrap();
        mark_dirty_session_flag(&dirty_flag_path).unwrap();

        let mut engine = Engine::new("bad");
        let status = recover_from_dirty_session(&mut engine, &autosave_path, &dirty_flag_path);

        assert_eq!(status, RecoveryStatus::AutosaveParseFailed);
        assert!(fs::metadata(&dirty_flag_path).is_ok());

        let _ = fs::remove_file(&autosave_path);
        let _ = fs::remove_file(&dirty_flag_path);
    }

    #[test]
    fn dirty_flag_helpers_mark_and_clear() {
        let path = temp_file("p9_dirty_flag_helpers");

        mark_dirty_session_flag(&path).unwrap();
        assert!(fs::metadata(&path).is_ok());

        clear_dirty_session_flag(&path).unwrap();
        assert!(fs::metadata(&path).is_err());
    }

    #[test]
    fn default_paths_point_to_temp_directory() {
        let autosave_path = default_autosave_path();
        let dirty_flag_path = default_dirty_flag_path();

        assert!(autosave_path.starts_with(std::env::temp_dir()));
        assert!(dirty_flag_path.starts_with(std::env::temp_dir()));
    }
}
