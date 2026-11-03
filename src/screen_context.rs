//! Volna's local screen context. Only an intentional contextual shortcut
//! captures a screen; ordinary dictation reads the current app's dictionary.
//! Retained dictations keep their screen evidence in local History.

mod engine;
mod evidence;
mod phonetic;
#[cfg(target_os = "macos")]
mod probe;
mod reference;
#[cfg(target_os = "macos")]
pub(crate) use probe::execute as run_probe;
#[cfg(target_os = "macos")]
pub(crate) use probe::status as probe_status;
#[cfg(target_os = "macos")]
mod reader;
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
    learn: bool,
    started: AtomicBool,
    state: Mutex<Option<ScreenText>>,
    ready: Condvar,
    reference: String,
    generation: u64,
    trace: Mutex<Option<crate::history::context::Draft>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ScreenText {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub elapsed_ms: u64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default, skip_serializing)]
    pub screenshot_jpeg: Option<String>,
    #[serde(default)]
    pub screenshot_note: Option<String>,
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
    begin_with_learning(context, capture, true)
}

fn begin_with_learning(context: &ContextSnapshot, capture: bool, learn: bool) -> ContextSnapshot {
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
        reference: store::text(&scope),
        generation: store::generation(),
        trace: Mutex::new(None),
        scope,
        application,
        pid,
        capture,
        learn,
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
                        if result.error.is_none()
                            && session.0.learn
                            && settings().enabled
                            && settings().learn
                        {
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
                error: Some("Screen reader is busy; using saved app text".into()),
                ..Default::default()
            });
            self.0.ready.notify_all();
        }
    }

    fn resolve(&self) -> (ScreenText, String) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        if self.0.started.load(Ordering::Acquire) && state.is_none() {
            state = self
                .0
                .ready
                .wait_timeout_while(state, Duration::from_millis(450), |s| s.is_none())
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        let screen = state.clone().unwrap_or_else(|| ScreenText {
            error: self.0.capture.then(|| {
                if self.0.started.load(Ordering::Acquire) {
                    "Screen reading did not finish before correction; saved app text used".into()
                } else {
                    "Screen reading was not started".into()
                }
            }),
            ..Default::default()
        });
        let reference = reference::combine(&screen.text, &self.0.reference);
        (screen, reference)
    }
}

pub fn correct(transcript: &str, context: &ContextSnapshot, cancelled: &AtomicBool) -> String {
    let Some(session) = context.screen.as_ref() else {
        return transcript.into();
    };
    if !settings().enabled || cancelled.load(Ordering::Acquire) {
        session.trace(
            &ScreenText::default(),
            "",
            transcript,
            transcript,
            "Screen context is disabled",
            0,
        );
        return transcript.into();
    }
    let started = Instant::now();
    if let Err(error) = store::expire() {
        tracing::warn!(%error, "Could not expire app vocabulary");
    }
    let (screen, reference) = session.resolve();
    if let Some(error) = &screen.error {
        engine::set_note(error);
    }
    if reference.is_empty() {
        session.trace(
            &screen,
            &reference,
            transcript,
            transcript,
            "No screen or saved text available; transcript unchanged",
            0,
        );
        return transcript.into();
    }
    if !settings().rewrite {
        session.trace(
            &screen,
            &reference,
            transcript,
            transcript,
            "Local correction is off; transcript unchanged",
            0,
        );
        return transcript.into();
    }
    if transcript.chars().count() > 5000 {
        session.trace(
            &screen,
            &reference,
            transcript,
            transcript,
            "Transcript exceeds local model limit; transcript unchanged",
            started.elapsed().as_millis() as u64,
        );
        return transcript.into();
    }
    match engine::correct(transcript, &reference, cancelled) {
        Ok(text) => {
            session.trace(
                &screen,
                &reference,
                transcript,
                &text,
                if text == transcript {
                    "No spelling changes selected from reference text"
                } else {
                    "Context spelling corrections applied"
                },
                started.elapsed().as_millis() as u64,
            );
            tracing::info!(
                latency_ms = started.elapsed().as_millis() as u64,
                ocr_ms = screen.elapsed_ms,
                context_chars = reference.chars().count(),
                "Volna applied local screen context"
            );
            text
        }
        Err(error) => {
            session.trace(
                &screen,
                &reference,
                transcript,
                transcript,
                &error,
                started.elapsed().as_millis() as u64,
            );
            engine::set_note(&error);
            transcript.into()
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
    fn volna_ocr_bytes(bytes: *const u8, count: usize) -> *mut std::ffi::c_char;
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
        let mut screen = read_native(volna_screen_capture(pid));
        if screen.error.is_none() {
            match reader::recognize(&screen) {
                Ok(ocr) => {
                    screen.text = ocr.text;
                    screen.elapsed_ms = ocr.elapsed_ms;
                    screen.error = ocr.error;
                }
                Err(error) => screen.error = Some(error),
            }
        }
        screen
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
    /// Show local OCR, engine, and app-context status (no screen capture).
    Status,
    /// List remembered application text (also preserves legacy dictionaries).
    Dictionaries,
    /// Test OCR against a supplied image; does not capture the screen or learn.
    Ocr { image: std::path::PathBuf },
    /// Test the running app's real capture/correction path. No history is saved.
    Probe {
        /// Test saved app text without taking a screenshot (like Option).
        #[arg(long)]
        dictionary: bool,
        /// Permit this explicit probe to remember text, like Right Command.
        #[arg(long)]
        learn: bool,
        #[arg(long, default_value = "")]
        transcript: String,
    },
    /// Test local correction on supplied text, without learning or capturing.
    Rewrite {
        transcript: String,
        #[arg(long, default_value = "")]
        screen: String,
        /// Reuse this application's saved text without taking a screenshot.
        #[arg(long)]
        application: Option<String>,
    },
}

#[cfg(target_os = "macos")]
pub fn run(command: &Command) -> color_eyre::Result<()> {
    match command {
        Command::Status => probe::run_status_cli()?,
        Command::Dictionaries => println!("{}", serde_json::to_string_pretty(&dictionaries())?),
        Command::Probe {
            dictionary,
            learn,
            transcript,
        } => probe::run_cli(!dictionary, *learn, transcript)?,
        Command::Ocr { image } => {
            if image == std::path::Path::new("-") {
                use std::io::Read;
                let mut bytes = Vec::new();
                std::io::stdin()
                    .take(6 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)?;
                color_eyre::eyre::ensure!(
                    bytes.len() <= 6 * 1024 * 1024,
                    "OCR input exceeds 6 MiB"
                );
                // SAFETY: bytes stay alive through the synchronous bridge call.
                let result = unsafe { read_native(volna_ocr_bytes(bytes.as_ptr(), bytes.len())) };
                println!("{}", serde_json::to_string(&result)?);
                return Ok(());
            }
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
            let saved = application.as_deref().map(store::text).unwrap_or_default();
            let reference = reference::combine(screen, &saved);
            let result = engine::correct(transcript, &reference, &AtomicBool::new(false));
            shutdown();
            println!("{}", result.map_err(|e| color_eyre::eyre::eyre!(e))?);
        }
    }
    Ok(())
}
