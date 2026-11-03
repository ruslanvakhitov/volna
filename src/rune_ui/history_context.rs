//! Evidence from the same dictation job, with honest unchanged/failure states.
use super::{
    Ui,
    controls::{ButtonKind, Paint, button},
    lists::section_label,
};
use crate::{desktop::i18n::tr, history::context::Evidence};
use rune::{El, col, el, text};

pub(super) fn transcript(p: Paint, key: &str, label: &str, value: &str) -> El {
    col()
        .key(key)
        .gap(6.0)
        .child(section_label(p, "label", label))
        .child(
            text(if value.is_empty() {
                tr("No text")
            } else {
                value
            })
            .key("value")
            .font(p.k.ty.body)
            .color(p.text())
            .wrap(),
        )
}

pub(super) fn detail(ui: &Ui, p: Paint, id: u64, c: &Evidence) -> El {
    let mut section = col()
        .key("screen-evidence")
        .gap(14.0)
        .child(section_label(p, "title", tr("Screen context · evidence")))
        .child(
            text(format!(
                "{} · {} ms OCR · {} ms correction\n{}\n{}",
                c.application_id, c.ocr_ms, c.correction_ms, c.ocr_status, c.correction_status
            ))
            .key("status")
            .font(p.k.ty.small)
            .color(p.muted())
            .wrap(),
        );
    if c.contextual {
        if let Some((_, image)) = ui
            .history
            .context_image
            .as_ref()
            .filter(|(entry, _)| *entry == id)
        {
            if image.error().is_some() {
                section = section.child(
                    text(tr("The saved screenshot could not be opened."))
                        .key("image-error")
                        .font(p.k.ty.small)
                        .color(p.muted())
                        .wrap(),
                );
            } else {
                section = section.child(
                    el().key("screenshot")
                        .w_full()
                        .h(170.0)
                        .radius(p.k.radius.md)
                        .fill(p.k.c.layer_01)
                        .pixels_fit(image.pixels(), rune::SurfaceFit::Fit),
                );
            }
            section = section.child(button(
                p,
                "open-screenshot",
                tr("Open screenshot"),
                ButtonKind::Secondary,
                move |ui: &mut Ui, _| {
                    if let Some(path) = ui.history.screenshot_path(id) {
                        if let Err(error) =
                            crate::desktop::opencode_catalog::open_url(&path.display().to_string())
                        {
                            super::show_error(ui, error.to_string());
                        }
                    }
                },
            ));
        } else {
            section = section.child(
                text(
                    c.screenshot_note
                        .as_deref()
                        .unwrap_or(tr("No screenshot available for this entry.")),
                )
                .key("no-screenshot")
                .font(p.k.ty.small)
                .color(p.muted())
                .wrap(),
            );
        }
        section = section.child(transcript(
            p,
            "ocr",
            tr("Text extracted from screen"),
            &c.ocr_text,
        ));
    }
    if !c.reference_text.is_empty() && (!c.contextual || c.reference_text != c.ocr_text) {
        section = section.child(transcript(
            p,
            "context-text",
            tr("Text used for context"),
            &c.reference_text,
        ));
    }
    section = section.child(transcript(
        p,
        "local-result",
        tr("After local context correction"),
        &c.after,
    ));
    section.child(
        text(format!(
            "{}: {}",
            tr("Saved context characters at start"),
            c.saved_context_chars
        ))
        .key("saved-terms")
        .font(p.k.ty.small)
        .color(p.muted())
        .wrap(),
    )
}
