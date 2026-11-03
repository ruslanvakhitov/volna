//! Plain screen text is the reference. Spelling proposals are matched to the
//! spoken transcript; there is no ranked bag of extracted screen keywords.
use std::collections::HashSet;
use std::sync::OnceLock;

pub(super) const MAX_CHARS: usize = 8000;

pub(super) fn combine(screen: &str, saved: &str) -> String {
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    let mut remaining = MAX_CHARS;
    for line in screen.lines().chain(saved.lines()) {
        let line = line.trim().trim_start_matches('#').trim();
        if line.is_empty() || !seen.insert(line.to_owned()) || remaining == 0 {
            continue;
        }
        let line: String = line.chars().take(remaining).collect();
        remaining = remaining.saturating_sub(line.chars().count() + 1);
        lines.push(line);
    }
    lines.join("\n")
}

pub(super) fn words(text: &str) -> Vec<String> {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| regex::Regex::new(r"[\p{L}\p{N}_-]+").unwrap());
    pattern.find_iter(text).map(|m| m.as_str().into()).collect()
}

pub(super) fn spellings(reference: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    words(reference)
        .into_iter()
        .filter(|word| {
            (3..=48).contains(&word.chars().count())
                && !word.chars().any(char::is_numeric)
                && seen.insert(word.clone())
        })
        .collect()
}

fn acronym(text: &str) -> bool {
    (3..=6).contains(&text.len()) && text.bytes().all(|c| c.is_ascii_uppercase())
}

/// A repeated acronym in a matching reference phrase is stronger evidence
/// than one different letter from speech recognition. Require a unique match,
/// two surrounding words in the same reference line and at least two sightings.
pub(super) fn restore_acronyms(text: &str, reference: &str) -> String {
    let reference_words = words(reference);
    let spoken = words(text);
    let surrounding: HashSet<_> = spoken
        .iter()
        .filter(|w| !acronym(w) && w.chars().count() >= 4)
        .map(|w| w.to_lowercase())
        .collect();
    let terms = spellings(reference);
    let mut edits = Vec::new();
    for heard in spoken.iter().filter(|w| acronym(w)) {
        if reference_words.contains(heard) {
            continue;
        }
        let matches: Vec<_> = terms
            .iter()
            .filter(|term| {
                acronym(term)
                    && super::phonetic::close(heard, term)
                    && reference_words.iter().filter(|w| *w == *term).count() >= 2
                    && reference.lines().any(|line| {
                        let line_words = words(line);
                        line_words.contains(term)
                            && line_words
                                .iter()
                                .map(|w| w.to_lowercase())
                                .filter(|w| surrounding.contains(w))
                                .collect::<HashSet<_>>()
                                .len()
                                >= 2
                    })
            })
            .collect();
        if let [term] = matches.as_slice() {
            edits.push((heard.as_str(), term.as_str()));
        }
    }
    let pattern = regex::Regex::new(r"[\p{L}\p{N}_-]+").unwrap();
    pattern
        .replace_all(text, |caps: &regex::Captures<'_>| {
            edits
                .iter()
                .find(|(heard, _)| *heard == &caps[0])
                .map(|(_, term)| (*term).to_owned())
                .unwrap_or_else(|| caps[0].to_owned())
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_preserves_sentences_without_keyword_ranking_or_heading_markers() {
        assert_eq!(
            combine(
                "## SLP AI: система бизнеса\n\nCodex",
                "Codex\nСобытие SLP Alumni"
            ),
            "SLP AI: система бизнеса\nCodex\nСобытие SLP Alumni"
        );
        assert!(combine(&"я".repeat(MAX_CHARS + 1), "saved").chars().count() <= MAX_CHARS);
    }

    #[test]
    fn relevant_spelling_is_available_beyond_the_old_keyword_limit() {
        let mut reference = String::new();
        for i in 0..140 {
            reference.push_str(&format!(
                "Menu{}{}\n",
                (b'A' + i / 26) as char,
                (b'A' + i % 26) as char
            ));
        }
        reference.push_str("Codex — developer tools");
        let candidates =
            super::super::phonetic::candidates("открой кодекс", &spellings(&reference));
        assert!(
            candidates
                .iter()
                .any(|c| c.heard == "кодекс" && c.term == "Codex")
        );
    }

    #[test]
    fn repeated_acronym_in_matching_phrase_is_corrected_without_capitalizing_prose() {
        assert_eq!(
            restore_acronyms(
                "Обсудим CLP и план встречи CLP в 15 часов",
                "План встречи SLP: отчёты\nSLP сообщество"
            ),
            "Обсудим SLP и план встречи SLP в 15 часов"
        );
    }

    #[test]
    fn acronyms_need_unique_repeated_evidence_and_matching_phrase() {
        for reference in [
            "SLP AI",
            "SLP AI\nSLP Alumni",
            "CLP — система бизнеса\nSLP — система бизнеса\nSLP Alumni",
            "SLP — система бизнеса\nSLP Alumni\nCLM — система бизнеса\nCLM Alumni",
        ] {
            let text = "CLP — система бизнеса в 15 часов";
            assert_eq!(restore_acronyms(text, reference), text, "{reference}");
        }
    }
}
