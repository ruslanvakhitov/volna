use super::*;

#[test]
fn vocabulary_excludes_addresses_numbers_and_ui_but_keeps_names() {
    let terms = store::extract_terms(
        "Telegram Settings Send Привет Сегодня ConfidAI Moat8 Codex GigaAM Qwen3 example@mail.com https://secret.test 123456 Сергей Александрович",
    );
    assert!(terms.contains(&"ConfidAI".into()));
    assert!(terms.contains(&"Codex".into()));
    assert!(terms.contains(&"Сергей".into()));
    assert!(
        !terms
            .iter()
            .any(|t| t.contains('@') || t.contains("secret") || t.contains("123456"))
    );
    assert!(!terms.contains(&"Settings".into()));
}

#[test]
fn ordinary_dictation_resolves_only_its_fixed_dictionary_without_ocr() {
    let session = Session(Arc::new(SessionInner {
        generation: 0,
        trace: Mutex::new(None),
        scope: "org.telegram.desktop".into(),
        application: "Telegram".into(),
        pid: 123,
        capture: false,
        started: AtomicBool::new(false),
        state: Mutex::new(None),
        ready: Condvar::new(),
        terms: vec!["ConfidAI".into()],
    }));
    session.start();
    assert!(!session.0.started.load(Ordering::Acquire));
    let (screen, terms) = session.resolve();
    assert!(screen.text.is_empty());
    assert_eq!(terms, vec!["ConfidAI"]);
}

#[test]
fn fresh_screen_terms_precede_saved_terms_and_merge_without_duplicates() {
    let session = Session(Arc::new(SessionInner {
        generation: 0,
        trace: Mutex::new(None),
        scope: "test.editor".into(),
        application: "Editor".into(),
        pid: 0,
        capture: true,
        started: AtomicBool::new(true),
        state: Mutex::new(Some(ScreenText {
            text: "ConfidAI Codex".into(),
            ..Default::default()
        })),
        ready: Condvar::new(),
        terms: vec!["ConfidAI".into(), "Docker".into()],
    }));
    let (_, terms) = session.resolve();
    assert_eq!(terms.iter().filter(|t| t.as_str() == "ConfidAI").count(), 1);
    assert!(terms.contains(&"Docker".into()));
    assert_ne!(
        session,
        Session(Arc::new(SessionInner {
            generation: 0,
            trace: Mutex::new(None),
            scope: "other".into(),
            application: "Other".into(),
            pid: 0,
            capture: false,
            started: AtomicBool::new(false),
            state: Mutex::new(None),
            ready: Condvar::new(),
            terms: vec![],
        }))
    );
}

#[test]
fn exact_correction_preserves_punctuation_numbers_and_partial_words() {
    assert_eq!(
        store::exact_spellings(
            "открой confidai, 120 и codex-tools.",
            &["ConfidAI".into(), "Codex".into()]
        ),
        "открой ConfidAI, 120 и codex-tools."
    );
}

#[test]
fn history_records_the_used_screen_even_when_later_state_changes() {
    let session = Session(Arc::new(SessionInner {
        scope: "test.fixture".into(),
        application: "Fixture".into(),
        pid: 0,
        capture: true,
        started: AtomicBool::new(true),
        state: Mutex::new(None),
        ready: Condvar::new(),
        terms: vec![],
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
        &["Codex".into()],
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
        started: AtomicBool::new(false),
        state: Mutex::new(None),
        ready: Condvar::new(),
        terms: vec!["Codex".into()],
        generation: 0,
        trace: Mutex::new(None),
    }));
    let (screen, terms) = session.resolve();
    session.trace(&screen, &terms, "кодекс", "кодекс", "unchanged", 0);
    let draft = session.history("кодекс");
    assert!(draft.evidence.ocr_status.contains("not started"));
    assert!(draft.jpeg.is_none());
    assert_eq!(draft.evidence.saved_terms, 1);
}
