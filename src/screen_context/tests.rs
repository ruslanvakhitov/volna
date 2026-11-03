use super::*;

#[test]
fn ordinary_dictation_resolves_only_its_fixed_dictionary_without_ocr() {
    let session = Session(Arc::new(SessionInner {
        generation: 0,
        trace: Mutex::new(None),
        scope: "org.telegram.desktop".into(),
        application: "Telegram".into(),
        pid: 123,
        capture: false,
        learn: true,
        started: AtomicBool::new(false),
        state: Mutex::new(None),
        ready: Condvar::new(),
        reference: "ConfidAI project".into(),
    }));
    session.start();
    assert!(!session.0.started.load(Ordering::Acquire));
    let (screen, reference) = session.resolve();
    assert!(screen.text.is_empty());
    assert_eq!(reference, "ConfidAI project");
}

#[test]
fn fresh_screen_text_precedes_saved_context_and_preserves_sentences() {
    let session = Session(Arc::new(SessionInner {
        generation: 0,
        trace: Mutex::new(None),
        scope: "test.editor".into(),
        application: "Editor".into(),
        pid: 0,
        capture: true,
        learn: true,
        started: AtomicBool::new(true),
        state: Mutex::new(Some(ScreenText {
            text: "ConfidAI Codex\nСобытие SLP Alumni".into(),
            ..Default::default()
        })),
        ready: Condvar::new(),
        reference: "ConfidAI Codex\nDocker project".into(),
    }));
    let (_, reference) = session.resolve();
    assert_eq!(
        reference,
        "ConfidAI Codex\nСобытие SLP Alumni\nDocker project"
    );
    assert_ne!(
        session,
        Session(Arc::new(SessionInner {
            generation: 0,
            trace: Mutex::new(None),
            scope: "other".into(),
            application: "Other".into(),
            pid: 0,
            capture: false,
            learn: true,
            started: AtomicBool::new(false),
            state: Mutex::new(None),
            ready: Condvar::new(),
            reference: String::new(),
        }))
    );
}

#[test]
fn history_records_the_used_screen_even_when_later_state_changes() {
    let session = Session(Arc::new(SessionInner {
        scope: "test.fixture".into(),
        application: "Fixture".into(),
        pid: 0,
        capture: true,
        learn: true,
        started: AtomicBool::new(true),
        state: Mutex::new(None),
        ready: Condvar::new(),
        reference: String::new(),
        generation: 0,
        trace: Mutex::new(None),
    }));
    let screen = ScreenText {
        text: "Codex".into(),
        elapsed_ms: 12,
        ..Default::default()
    };
    session.trace(
        &screen,
        "Codex — developer tools",
        "кодекс 15",
        "Codex 15",
        "applied",
        20,
    );
    *session.0.state.lock().unwrap() = Some(ScreenText {
        text: "Unrelated later screen".into(),
        ..Default::default()
    });
    let evidence = session.history("кодекс 15").evidence;
    assert_eq!(evidence.ocr_text, "Codex");
    assert_eq!(evidence.reference_text, "Codex — developer tools");
    assert_eq!(evidence.before, "кодекс 15");
    assert_eq!(evidence.after, "Codex 15");
    assert_eq!(evidence.application_id, "test.fixture");
}

#[test]
fn incomplete_ocr_is_visible_as_fallback_not_a_success() {
    let session = Session(Arc::new(SessionInner {
        scope: "test.fixture".into(),
        application: "Fixture".into(),
        pid: 0,
        capture: true,
        learn: true,
        started: AtomicBool::new(false),
        state: Mutex::new(None),
        ready: Condvar::new(),
        reference: "Codex".into(),
        generation: 0,
        trace: Mutex::new(None),
    }));
    let (screen, reference) = session.resolve();
    session.trace(&screen, &reference, "кодекс", "кодекс", "unchanged", 0);
    let draft = session.history("кодекс");
    assert!(draft.evidence.ocr_status.contains("not started"));
    assert!(draft.jpeg.is_none());
    assert_eq!(draft.evidence.saved_context_chars, 5);
}
