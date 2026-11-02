//! Vision is isolated from the GUI/capture executor. Pixels travel only through
//! anonymous pipes; the child does no screen capture, learning or logging.
use super::ScreenText;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::io::{Read, Write};
use std::process::Stdio;
use std::time::{Duration, Instant};

pub(super) fn recognize(screen: &ScreenText) -> Result<ScreenText, String> {
    let encoded = screen
        .screenshot_jpeg
        .as_ref()
        .ok_or("No screen image to recognize")?;
    let jpeg = STANDARD
        .decode(encoded)
        .map_err(|_| "Screen image encoding is invalid")?;
    let binary = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = crate::desktop::background_process::command(binary)
        .args(["context", "ocr", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start the screen reader: {e}"))?;
    let mut input = child
        .stdin
        .take()
        .ok_or("Screen reader input is unavailable")?;
    let output = child
        .stdout
        .take()
        .ok_or("Screen reader output is unavailable")?;
    let writer = std::thread::spawn(move || input.write_all(&jpeg));
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        output
            .take(256 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let completed = loop {
        match child.try_wait() {
            Ok(Some(_)) => break true,
            Err(_) => break false,
            Ok(None) if Instant::now() >= deadline => break false,
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    if !completed {
        let _ = child.kill();
    }
    let _ = child.wait();
    let _ = writer.join();
    let output = reader
        .join()
        .map_err(|_| "Screen reader stopped")?
        .map_err(|e| e.to_string())?;
    if !completed {
        return Err("Vision OCR timed out after 5 seconds".into());
    }
    if output.len() > 256 * 1024 {
        return Err("Screen reader response exceeds its limit".into());
    }
    serde_json::from_slice(&output).map_err(|_| "Screen reader returned an invalid result".into())
}
