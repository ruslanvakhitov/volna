//! The screen and correction evidence belonging to one retained dictation.
//! JPEGs follow History's retention, Delete and Clear, under a separate cap.
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub const MAX_SCREENSHOT_BYTES: u64 = 6 * 1024 * 1024;
pub const MAX_TOTAL_SCREENSHOT_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Evidence {
    pub contextual: bool,
    pub application_id: String,
    pub ocr_text: String,
    pub ocr_status: String,
    pub ocr_ms: u64,
    pub correction_status: String,
    pub correction_ms: u64,
    pub before: String,
    pub after: String,
    pub reference_text: String,
    pub saved_context_chars: usize,
    pub terms: Vec<String>,
    pub saved_terms: usize,
    pub screenshot_bytes: u64,
    pub screenshot_note: Option<String>,
}

impl Evidence {
    pub(super) fn bounded(mut self) -> Self {
        self.application_id = super::truncated(&self.application_id, super::MAX_LABEL_BYTES);
        self.ocr_text = super::truncated(&self.ocr_text, super::MAX_TEXT_BYTES);
        self.before = super::truncated(&self.before, super::MAX_TEXT_BYTES);
        self.after = super::truncated(&self.after, super::MAX_TEXT_BYTES);
        self.reference_text = super::truncated(&self.reference_text, super::MAX_TEXT_BYTES);
        self.ocr_status = super::truncated(&self.ocr_status, super::MAX_FALLBACK_BYTES);
        self.correction_status =
            super::truncated(&self.correction_status, super::MAX_FALLBACK_BYTES);
        self.screenshot_note = self
            .screenshot_note
            .map(|s| super::truncated(&s, super::MAX_FALLBACK_BYTES));
        self.terms = self
            .terms
            .into_iter()
            .take(96)
            .map(|s| super::truncated(&s, 256))
            .collect();
        self.screenshot_bytes = 0;
        self
    }

    pub(super) fn text_bytes(&self) -> usize {
        self.ocr_text.len()
            + self.before.len()
            + self.after.len()
            + self.reference_text.len()
            + self.terms.iter().map(String::len).sum::<usize>()
    }
}

#[derive(Clone, Debug)]
pub struct Draft {
    pub evidence: Evidence,
    pub jpeg: Option<Arc<[u8]>>,
}

pub(super) fn directory(history: &Path) -> PathBuf {
    history.with_file_name("history-screens")
}
pub(super) fn path(directory: &Path, id: u64) -> PathBuf {
    directory.join(format!("{id}.jpg"))
}

pub(super) fn write(directory: &Path, id: u64, jpeg: &[u8]) -> io::Result<u64> {
    if jpeg.len() as u64 > MAX_SCREENSHOT_BYTES || !jpeg.starts_with(&[0xff, 0xd8, 0xff]) {
        return Err(io::Error::other("Screenshot is invalid or exceeds 6 MiB"));
    }
    fs::create_dir_all(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    let target = path(directory, id);
    let temporary = target.with_extension("jpg.tmp");
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        super::restrict_to_owner(&file)?;
        file.write_all(jpeg)?;
        file.sync_all()?;
        fs::rename(&temporary, target)?;
        Ok(jpeg.len() as u64)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub(super) fn sweep(directory: &Path, below: u64, keep: impl Fn(u64) -> bool) {
    let Ok(files) = fs::read_dir(directory) else {
        return;
    };
    for file in files.flatten() {
        let name = file.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(id) = name
            .strip_suffix(".jpg")
            .or_else(|| name.strip_suffix(".jpg.tmp"))
            .and_then(|s| s.parse::<u64>().ok())
        else {
            continue;
        };
        if id < below && (name.ends_with(".tmp") || !keep(id)) {
            if let Err(error) = fs::remove_file(file.path()) {
                tracing::warn!(%error, id, "could not delete a History screenshot");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{HistoryDraft, HistoryKind, HistoryRetention, HistoryStore};

    fn draft() -> HistoryDraft {
        HistoryDraft {
            kind: HistoryKind::Dictation,
            raw_text: "кодекс 15".into(),
            final_text: "Codex 15".into(),
            application: Some("Fixture".into()),
            processing: None,
            audio_ms: 500,
            inference_ms: 20,
            total_ms: 80,
            recording: None,
            screen_context: Some(Draft {
                evidence: Evidence {
                    contextual: true,
                    ocr_text: "Codex".into(),
                    reference_text: "Codex\nSaved project context".into(),
                    before: "кодекс 15".into(),
                    after: "Codex 15".into(),
                    ..Default::default()
                },
                jpeg: Some(Arc::from([0xff, 0xd8, 0xff, 0xe0, 0, 0, 0xff, 0xd9])),
            }),
        }
    }

    #[test]
    fn screen_evidence_follows_history_off_delete_clear_expiry_and_byte_cap() {
        let root = std::env::temp_dir().join(format!(
            "volna-history-evidence-{}-{}",
            std::process::id(),
            crate::history::now_ms()
        ));
        let history = root.join("history.json");
        let mut store = HistoryStore::open(history.clone(), HistoryRetention::Off, 100);
        assert_eq!(store.record(draft(), 100).unwrap(), None);
        assert!(!directory(&history).exists());
        store.set_retention(HistoryRetention::Day, 100).unwrap();
        let first = store.record(draft(), 100).unwrap().unwrap();
        let screenshot = store.screenshot_path(first).unwrap();
        assert_eq!(
            fs::read(&screenshot).unwrap(),
            draft().screen_context.unwrap().jpeg.unwrap().as_ref()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&screenshot).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(directory(&history))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        let loaded = crate::history::read_snapshot(&history).unwrap();
        assert_eq!(loaded[0].screen_context.as_ref().unwrap().ocr_text, "Codex");
        assert_eq!(
            loaded[0].screen_context.as_ref().unwrap().reference_text,
            "Codex\nSaved project context"
        );
        store.screenshot_cap = 8;
        let second = store.record(draft(), 101).unwrap().unwrap();
        assert!(!screenshot.exists());
        assert!(store.entry(first).is_some(), "cap keeps the text record");
        assert!(
            store
                .entry(first)
                .unwrap()
                .screen_context
                .as_ref()
                .unwrap()
                .screenshot_note
                .is_some()
        );
        let second_path = store.screenshot_path(second).unwrap();
        store.delete(second).unwrap();
        assert!(!second_path.exists());
        let id = store.record(draft(), 102).unwrap().unwrap();
        let path = store.screenshot_path(id).unwrap();
        store.clear().unwrap();
        assert!(!path.exists());
        let id = store.record(draft(), 103).unwrap().unwrap();
        let path = store.screenshot_path(id).unwrap();
        drop(store);
        let store = HistoryStore::open(history, HistoryRetention::Day, 103 + 86_400_001);
        assert_eq!(store.len(), 0);
        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn old_history_entries_deserialize_without_context() {
        let mut value = serde_json::to_value(crate::history::HistoryEntry {
            id: 1,
            timestamp_ms: 1,
            kind: HistoryKind::Dictation,
            raw_text: "before".into(),
            final_text: "after".into(),
            application: None,
            processing: None,
            audio_ms: 1,
            inference_ms: 1,
            total_ms: 1,
            recording_bytes: 0,
            screen_context: None,
        })
        .unwrap();
        value.as_object_mut().unwrap().remove("screen_context");
        let entry: crate::history::HistoryEntry = serde_json::from_value(value).unwrap();
        assert!(entry.screen_context.is_none());
    }
}
