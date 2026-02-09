use std::fs;
use std::io;
use std::path::Path;

use p9_core::engine::Engine;
use p9_storage::project::ProjectEnvelope;

use crate::runtime::TransportSnapshot;

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

#[cfg(test)]
mod tests {
    use super::{AutosaveManager, AutosavePolicy};
    use crate::runtime::{SyncMode, TransportSnapshot};
    use p9_core::engine::Engine;
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
}
