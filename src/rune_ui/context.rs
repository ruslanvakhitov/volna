//! The screen-context controls and inspectable application dictionaries.
use super::{
    Ui,
    controls::{
        ButtonKind, CardRow, Paint, button, icon_row, icon_toggle_row, section as card_section,
    },
    icons::{BOOK, KEYBOARD, WAVE},
};
use crate::desktop::i18n::tr;
use rune::{El, col, text};

#[derive(Default)]
pub(super) struct State {
    pub expanded: bool,
    pub armed: Option<String>,
    pub settings: crate::screen_context::Settings,
}

fn update(ui: &mut Ui, edit: impl FnOnce(&mut crate::screen_context::Settings)) {
    let mut settings = if ui.live() {
        crate::screen_context::settings()
    } else {
        ui.context.settings
    };
    edit(&mut settings);
    ui.context.settings = settings;
    if ui.live() {
        if let Err(error) = crate::screen_context::save_settings(settings) {
            super::show_error(ui, error);
        }
    }
}

pub(super) fn section(ui: &Ui, p: Paint) -> Option<El> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    // Fixtures never inspect the user's dictionary or settings.
    let live = ui.live();
    let settings = if live {
        crate::screen_context::settings()
    } else {
        ui.context.settings
    };
    let mut rows: Vec<CardRow> = vec![
        icon_toggle_row(
            p,
            "screen-context-enabled",
            &WAVE,
            tr("Screen context"),
            tr("Right Command reads the screen; Option reuses this app’s dictionary"),
            settings.enabled,
            |ui| update(ui, |s| s.enabled = !s.enabled),
        )
        .into(),
        icon_toggle_row(
            p,
            "screen-context-learning",
            &BOOK,
            tr("Learn app vocabulary"),
            tr("App terms stay 30 days. Screenshots and OCR follow your History retention."),
            settings.learn,
            |ui| update(ui, |s| s.learn = !s.learn),
        )
        .into(),
        icon_toggle_row(
            p,
            "screen-context-rewrite",
            &WAVE,
            tr("Local spelling correction"),
            tr("A small Qwen model fixes names and terms after dictation"),
            settings.rewrite,
            |ui| update(ui, |s| s.rewrite = !s.rewrite),
        )
        .into(),
    ];
    if live && !crate::screen_context::permission(false) {
        rows.push(
            icon_row(
                p,
                "screen-context-permission",
                &KEYBOARD,
                tr("Screen Recording permission"),
                Some(tr(
                    "Allow Volna to read screen text when you hold Right Command",
                )),
                button(
                    p,
                    "screen-context-allow",
                    tr("Allow"),
                    ButtonKind::Secondary,
                    |_ui, _| {
                        crate::screen_context::permission(true);
                    },
                ),
            )
            .into(),
        );
    }
    let dictionaries = if live {
        crate::screen_context::dictionaries()
    } else {
        Vec::new()
    };
    let total: usize = dictionaries.iter().map(|d| d.terms.len()).sum();
    let summary = format!("{} · {}", dictionaries.len(), total);
    rows.push(
        icon_row(
            p,
            "screen-context-memory",
            &BOOK,
            tr("App dictionaries"),
            Some(if live {
                tr("Inspect learned terms or forget an app")
            } else {
                tr("Separate vocabulary for Telegram, your editor and every other app")
            }),
            button(
                p,
                "screen-context-inspect",
                &summary,
                ButtonKind::Secondary,
                |ui, _| {
                    ui.context.expanded = !ui.context.expanded;
                    ui.context.armed = None;
                },
            ),
        )
        .into(),
    );
    if ui.context.expanded {
        for dictionary in dictionaries {
            let scope = dictionary.scope.clone();
            let armed = ui.context.armed.as_deref() == Some(&scope);
            let preview = dictionary
                .terms
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(" · ");
            rows.push(
                icon_row(
                    p,
                    &format!("app-dictionary-{scope}"),
                    &BOOK,
                    &dictionary.application,
                    Some(&preview),
                    button(
                        p,
                        &format!("forget-{scope}"),
                        tr(if armed { "Confirm forget" } else { "Forget" }),
                        if armed {
                            ButtonKind::Danger
                        } else {
                            ButtonKind::Secondary
                        },
                        move |ui, _| {
                            if ui.context.armed.as_ref() == Some(&scope) {
                                if let Err(error) = crate::screen_context::forget(&scope) {
                                    super::show_error(ui, error);
                                }
                                ui.context.armed = None;
                            } else {
                                ui.context.armed = Some(scope.clone());
                            }
                        },
                    ),
                )
                .into(),
            );
        }
    }
    let mut content = col().gap(10.0).child(card_section(
        p,
        "screen-context",
        tr("Context that stays with you"),
        rows,
    ));
    if live {
        content = content.child(
            text(crate::screen_context::engine_status())
                .font(p.k.ty.small)
                .color(p.muted())
                .wrap(),
        );
    }
    Some(content)
}
