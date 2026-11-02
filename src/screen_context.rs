//! Volna's local screen context. Only an intentional contextual shortcut
//! captures a screen; ordinary dictation reads the current app's dictionary.
//! Pixels and OCR excerpts are transient. Only bounded, expiring terms persist.

mod engine;
mod phonetic;
mod store;
#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::dictation::context::ContextSnapshot;
use serde::{Deserialize, Serialize};

pub use store::{dictionaries, forget};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub learn: bool,
    pub rewrite: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            learn: true,
            rewrite: true,
        }
    }
}

static CONTEXTUAL: AtomicBool = AtomicBool::new(false);
pub fn contextual() -> bool {
    CONTEXTUAL.load(Ordering::Relaxed)
}

static SETTINGS: OnceLock<Mutex<Settings>> = OnceLock::new();

pub fn settings() -> Settings {
    *SETTINGS
        .get_or_init(|| {
            let saved = crate::app_paths::support_dir()
                .ok()
                .and_then(|p| std::fs::read(p.join("context-settings.json")).ok())
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            Mutex::new(saved)
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn save_settings(value: Settings) -> Result<(), String> {
    let path = crate::app_paths::support_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
    store::private_write(&path.join("context-settings.json"), &bytes)?;
    *SETTINGS
        .get()
        .expect("settings loaded before editing")
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = value;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct Session(Arc<SessionInner>);

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug)]
struct SessionInner {
    scope: String,
    application: String,
    pid: i32,
    capture: bool,
    started: AtomicBool,
    state: Mutex<Option<ScreenText>>,
    ready: Condvar,
    terms: Vec<String>,
    generation: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ScreenText {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub elapsed_ms: u64,
    #[serde(default)]
    pub error: Option<String>,
}

/// Reads only the foreground application's identity, not its UI or screen.
#[cfg(target_os = "macos")]
fn foreground() -> Option<(String, String, i32)> {
    use objc2_app_kit::NSWorkspace;
    objc2::rc::autoreleasepool(|_| {
        let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        let name = app.localizedName()?.to_string();
        let scope = app
            .bundleIdentifier()
            .map(|s| s.to_string())
            .unwrap_or_else(|| name.clone());
        Some((name, scope, app.processIdentifier()))
    })
}

/// Fixes the foreground application at press time, independently of later
/// context-monitor updates or queued dictations.
pub fn begin(context: &ContextSnapshot, capture: bool) -> ContextSnapshot {
    CONTEXTUAL.store(capture && settings().enabled, Ordering::Relaxed);
    let mut context = context.clone();
    context.screen = None;
    if !settings().enabled {
        return context;
    }
    #[cfg(target_os = "macos")]
    let identity = foreground();
    #[cfg(not(target_os = "macos"))]
    let identity: Option<(String, String, i32)> = None;
    let Some((application, scope, pid)) = identity else {
        return context;
    };
    if scope == "com.ruslan.volna" {
        return context;
    }
    if context.application.as_ref() != Some(&application) {
        context.browser_url = None;
        context.window_title = None;
    }
    context.application = Some(application.clone());
    context.screen = Some(Session(Arc::new(SessionInner {
        terms: store::terms(&scope),
        generation: store::generation(),
        scope,
        application,
        pid,
        capture,
        started: AtomicBool::new(false),
        state: Mutex::new(None),
        ready: Condvar::new(),
    })));
    context
}

impl Session {
    /// Called after the hold threshold, never on a Command tap or chord.
    pub fn start(&self) {
        if !self.0.capture || !settings().enabled || self.0.started.swap(true, Ordering::AcqRel) {
            return;
        }
        tracing::info!(scope = self.0.scope, "Volna screen context started");
        static WORKER: OnceLock<SyncSender<Session>> = OnceLock::new();
        let worker = WORKER.get_or_init(|| {
            let (tx, rx) = mpsc::sync_channel::<Session>(2);
            std::thread::Builder::new()
                .name("volna-screen-ocr".into())
                .spawn(move || {
                    while let Ok(session) = rx.recv() {
                        let result = capture_screen(session.0.pid);
                        tracing::info!(
                            ocr_ms = result.elapsed_ms,
                            success = result.error.is_none(),
                            "Volna screen context captured"
                        );
                        if result.error.is_none() && settings().enabled && settings().learn {
                            if let Err(error) = store::learn(
                                &session.0.scope,
                                &session.0.application,
                                &result.text,
                                session.0.generation,
                            ) {
                                tracing::warn!(%error, "Volna could not save app vocabulary");
                            }
                        }
                        *session.0.state.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
                        session.0.ready.notify_all();
                    }
                })
                .expect("screen context worker");
            tx
        });
        if worker.try_send(self.clone()).is_err() {
            *self.0.state.lock().unwrap_or_else(|e| e.into_inner()) = Some(ScreenText {
                error: Some("Screen reader is busy; using the app dictionary".into()),
                ..Default::default()
            });
            self.0.ready.notify_all();
        }
    }

    fn resolve(&self) -> (ScreenText, Vec<String>) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        if self.0.started.load(Ordering::Acquire) && state.is_none() {
            state = self
                .0
                .ready
                .wait_timeout_while(state, Duration::from_millis(450), |s| s.is_none())
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        let screen = state.clone().unwrap_or_default();
        let mut terms = store::extract_terms(&screen.text);
        for term in &self.0.terms {
            if !terms.iter().any(|t| t.eq_ignore_ascii_case(term)) {
                terms.push(term.clone());
            }
        }
        terms.truncate(96);
        (screen, terms)
    }
}

pub fn correct(transcript: &str, context: &ContextSnapshot, cancelled: &AtomicBool) -> String {
    let Some(session) = context.screen.as_ref() else {
        return transcript.into();
    };
    if !settings().enabled || cancelled.load(Ordering::Acquire) {
        return transcript.into();
    }
    let started = Instant::now();
    if let Err(error) = store::expire() {
        tracing::warn!(%error, "Could not expire app vocabulary");
    }
    let (screen, terms) = session.resolve();
    if let Some(error) = screen.error {
        engine::set_note(&error);
    }
    if terms.is_empty() {
        return transcript.into();
    }
    let corrected = store::exact_spellings(transcript, &terms);
    if !settings().rewrite {
        return corrected;
    }
    match engine::correct(&corrected, &screen.text, &terms, cancelled) {
        Ok(text) => {
            tracing::info!(
                latency_ms = started.elapsed().as_millis() as u64,
                ocr_ms = screen.elapsed_ms,
                terms = terms.len(),
                "Volna applied local screen context"
            );
            text
        }
        Err(error) => {
            engine::set_note(&error);
            corrected
        }
    }
}

pub fn engine_status() -> String {
    engine::status()
}
pub fn warmup() {
    engine::warmup();
}
pub fn shutdown() {
    engine::shutdown();
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn volna_screen_capture(pid: i32) -> *mut std::ffi::c_char;
    fn volna_screen_free(string: *mut std::ffi::c_char);
    fn volna_screen_permission(request: bool) -> bool;
    fn volna_ocr_file(path: *const std::ffi::c_char) -> *mut std::ffi::c_char;
}

pub fn permission(request: bool) -> bool {
    #[cfg(target_os = "macos")]
    // SAFETY: a synchronous CoreGraphics permission query/request.
    unsafe {
        volna_screen_permission(request)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        false
    }
}

#[cfg(target_os = "macos")]
fn read_native(pointer: *mut std::ffi::c_char) -> ScreenText {
    if pointer.is_null() {
        return ScreenText {
            error: Some("Screen reader returned no result".into()),
            ..Default::default()
        };
    }
    // SAFETY: the bridge allocates a NUL-terminated string; free it once.
    unsafe {
        let result = serde_json::from_slice(std::ffi::CStr::from_ptr(pointer).to_bytes())
            .unwrap_or_else(|_| ScreenText {
                error: Some("Screen reader returned an invalid result".into()),
                ..Default::default()
            });
        volna_screen_free(pointer);
        result
    }
}

fn capture_screen(pid: i32) -> ScreenText {
    #[cfg(target_os = "macos")]
    // SAFETY: called on the bounded OCR worker; no pointers cross the call.
    unsafe {
        read_native(volna_screen_capture(pid))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        ScreenText {
            error: Some("Screen context requires macOS".into()),
            ..Default::default()
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(clap::Subcommand)]
pub enum Command {
    /// Show local OCR, engine, and dictionary status (no screen capture).
    Status,
    /// List learned application dictionaries.
    Dictionaries,
    /// Test OCR against a supplied image; does not capture the screen or learn.
    Ocr { image: std::path::PathBuf },
    /// Test local correction on supplied text, without learning or capturing.
    Rewrite {
        transcript: String,
        #[arg(long, default_value = "")]
        screen: String,
        /// Reuse this application's saved vocabulary without taking a screenshot.
        #[arg(long)]
        application: Option<String>,
    },
}

#[cfg(target_os = "macos")]
pub fn run(command: &Command) -> color_eyre::Result<()> {
    match command {
        Command::Status => println!(
            "{}",
            serde_json::json!({"screen_permission":permission(false), "settings":settings(), "engine":engine_status(), "dictionaries":dictionaries()})
        ),
        Command::Dictionaries => println!("{}", serde_json::to_string_pretty(&dictionaries())?),
        Command::Ocr { image } => {
            let path = std::ffi::CString::new(image.to_string_lossy().as_bytes())?;
            // SAFETY: path remains alive; the bridge owns the returned string.
            let result = unsafe { read_native(volna_ocr_file(path.as_ptr())) };
            println!("{}", serde_json::to_string_pretty(&result)?);
            if let Some(error) = result.error {
                return Err(color_eyre::eyre::eyre!(error));
            }
        }
        Command::Rewrite {
            transcript,
            screen,
            application,
        } => {
            let mut terms = store::extract_terms(screen);
            if let Some(application) = application {
                for term in store::terms(application) {
                    if !terms.contains(&term) {
                        terms.push(term);
                    }
                }
            }
            let result = engine::correct(transcript, screen, &terms, &AtomicBool::new(false));
            shutdown();
            println!("{}", result.map_err(|e| color_eyre::eyre::eyre!(e))?);
        }
    }
    Ok(())
}
