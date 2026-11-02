use super::*;
use crate::control::{ControlCommand, ControlError, ControlReply, ControlRequest};

pub(crate) fn execute(
    capture: bool,
    learn: bool,
    transcript: String,
) -> Result<ControlReply, ControlError> {
    if crate::parakeet::dictation_busy() {
        return Err(ControlError::new(
            "busy",
            "Finish dictation before testing screen context",
        ));
    }
    let started = Instant::now();
    let context = begin_with_learning(&ContextSnapshot::default(), capture, learn);
    let session = context.screen.as_ref().ok_or_else(|| {
        ControlError::new(
            "no-target",
            "Keep a target app in front; screen context must be enabled",
        )
    })?;
    session.start();
    if capture {
        let state = session.0.state.lock().unwrap_or_else(|e| e.into_inner());
        drop(
            session
                .0
                .ready
                .wait_timeout_while(state, Duration::from_secs(18), |s| s.is_none())
                .unwrap_or_else(|e| e.into_inner()),
        );
    }
    let after = correct(&transcript, &context, &AtomicBool::new(false));
    let draft = session.history(&transcript);
    let report = serde_json::json!({"total_ms":started.elapsed().as_millis(),
        "evidence":draft.evidence,"screenshot_bytes":draft.jpeg.as_ref().map_or(0, |b| b.len()),"after":after});
    Ok(ControlReply::Done {
        message: report.to_string(),
        changed: learn && capture,
    })
}

pub(crate) fn status() -> ControlReply {
    ControlReply::Done {
        message: serde_json::json!({"running_app":true,
        "permission_process":"running-app", "screen_permission":permission(false),
        "settings":settings(),"engine":engine_status(),"dictionaries":dictionaries()})
        .to_string(),
        changed: false,
    }
}

pub(super) fn run_status_cli() -> color_eyre::Result<()> {
    if crate::local_api::running_pid().is_none() {
        println!(
            "{}",
            serde_json::json!({"running_app":false,"permission_process":"this-command",
            "screen_permission":permission(false),"settings":settings(),"engine":engine_status(),"dictionaries":dictionaries()})
        );
        return Ok(());
    }
    print_reply(ControlCommand::ScreenContextStatus)
}

pub(super) fn run_cli(capture: bool, learn: bool, transcript: &str) -> color_eyre::Result<()> {
    print_reply(ControlCommand::ScreenContextProbe {
        capture,
        learn,
        transcript: transcript.into(),
    })
}

fn print_reply(command: ControlCommand) -> color_eyre::Result<()> {
    let response = crate::local_api::call_control(&ControlRequest::new(command))?;
    match response.reply {
        ControlReply::Done { message, .. } => println!("{message}"),
        ControlReply::Error { message, .. } => color_eyre::eyre::bail!(message),
        _ => color_eyre::eyre::bail!("Unexpected screen probe response"),
    }
    Ok(())
}
