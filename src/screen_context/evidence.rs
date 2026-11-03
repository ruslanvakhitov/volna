use super::*;
use crate::history::context::{Draft, Evidence};
use base64::{Engine, engine::general_purpose::STANDARD};

impl Session {
    pub(super) fn trace(
        &self,
        screen: &ScreenText,
        reference: &str,
        before: &str,
        after: &str,
        status: &str,
        ms: u64,
    ) {
        let jpeg = screen
            .screenshot_jpeg
            .as_ref()
            .filter(|s| s.len() <= 8 * 1024 * 1024)
            .and_then(|s| STANDARD.decode(s).ok())
            .map(Arc::<[u8]>::from);
        let ocr_status = if !self.0.capture {
            "Option: saved application text; no screenshot requested".into()
        } else if let Some(error) = &screen.error {
            error.clone()
        } else if screen.text.is_empty() {
            "Screen read completed, but no text was recognized".into()
        } else {
            "Screen text was available to correction".into()
        };
        *self.0.trace.lock().unwrap_or_else(|e| e.into_inner()) = Some(Draft {
            evidence: Evidence {
                contextual: self.0.capture,
                application_id: self.0.scope.clone(),
                ocr_text: screen.text.clone(),
                ocr_status,
                ocr_ms: screen.elapsed_ms,
                correction_status: status.into(),
                correction_ms: ms,
                before: before.into(),
                after: after.into(),
                reference_text: reference.into(),
                saved_context_chars: self.0.reference.chars().count(),
                screenshot_note: screen.screenshot_note.clone(),
                ..Default::default()
            },
            jpeg,
        });
    }

    /// Snapshot exactly the screen/terms used by this job, never another job's
    /// current foreground context or a late OCR answer presented as used.
    pub fn history(&self, raw: &str) -> Draft {
        let trace = self.0.trace.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(draft) = trace.as_ref() {
            return draft.clone();
        }
        drop(trace);
        let mut screen = self
            .0
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_default();
        if self.0.capture && screen.screenshot_jpeg.is_none() && screen.error.is_none() {
            screen.error =
                Some("Screen evidence was unavailable when this output was saved".into());
        }
        self.trace(
            &screen,
            &self.0.reference,
            raw,
            raw,
            "Correction was bypassed for this dictation",
            0,
        );
        self.0
            .trace
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .expect("trace just recorded")
    }
}
