//! Candidate spans come from the transcript, never from generated text.
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(super) struct Candidate {
    pub heard: String,
    pub term: String,
}

fn normalized(text: &str) -> String {
    let mut value = String::new();
    for c in text.to_lowercase().chars() {
        value.push_str(match c {
            'а' => "a",
            'б' => "b",
            'в' => "v",
            'г' => "g",
            'д' => "d",
            'е' | 'ё' | 'э' => "e",
            'ж' => "zh",
            'з' => "z",
            'и' | 'й' | 'ы' => "i",
            'к' => "k",
            'л' => "l",
            'м' => "m",
            'н' => "n",
            'о' => "o",
            'п' => "p",
            'р' => "r",
            'с' => "s",
            'т' => "t",
            'у' => "u",
            'ф' => "f",
            'х' => "h",
            'ц' => "ts",
            'ч' => "ch",
            'ш' | 'щ' => "sh",
            'ю' => "iu",
            'я' => "ia",
            'ь' | 'ъ' => "",
            'c' | 'q' => "k",
            'x' => "ks",
            'w' => "v",
            'y' => "i",
            _ => {
                if c.is_ascii_alphanumeric() {
                    value.push(c);
                }
                continue;
            }
        });
    }
    value
        .replace("ph", "f")
        .replace("kk", "k")
        .replace("ee", "i")
        .replace("oo", "u")
}

fn distance(a: &str, b: &str) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.bytes().enumerate() {
        let mut row = vec![i + 1];
        for (j, y) in b.bytes().enumerate() {
            row.push(
                (previous[j + 1] + 1)
                    .min(row[j] + 1)
                    .min(previous[j] + usize::from(x != y)),
            );
        }
        previous = row;
    }
    previous[b.len()]
}

pub(super) fn close(heard: &str, term: &str) -> bool {
    let a = normalized(heard);
    let b = normalized(term);
    let acronym = |s: &str| (3..=6).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_uppercase());
    if acronym(heard) && acronym(term) && a.len() == b.len() {
        return distance(&a, &b) <= 1;
    }
    if a.len() < 4 || b.len() < 4 || a == b {
        return a == b && a.len() >= 3;
    }
    let min = a.len().min(b.len());
    let max = a.len().max(b.len());
    min * 3 >= max * 2
        && a.as_bytes().first() == b.as_bytes().first()
        && distance(&a, &b) <= (min / 4).clamp(1, 2)
}

pub(super) fn candidates(text: &str, terms: &[String]) -> Vec<Candidate> {
    let re = regex::Regex::new(r"[\p{L}\p{N}_-]+").unwrap();
    let words: Vec<_> = re.find_iter(text).collect();
    let mut candidates = Vec::new();
    for (i, word) in words.iter().enumerate() {
        for n in 1..=3 {
            let Some(end) = words.get(i + n - 1) else {
                break;
            };
            let heard = &text[word.start()..end.end()];
            if heard.chars().count() > 64 || heard.chars().any(|c| c.is_numeric()) {
                break;
            }
            for term in terms {
                if term.chars().any(|c| c.is_numeric()) {
                    continue;
                }
                if heard.to_lowercase() != term.to_lowercase()
                    && close(heard, term)
                    && !candidates
                        .iter()
                        .any(|c: &Candidate| c.heard == heard && c.term == *term)
                {
                    candidates.push(Candidate {
                        heard: heard.into(),
                        term: term.clone(),
                    });
                    if candidates.len() >= 32 {
                        return candidates;
                    }
                }
            }
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn russian_pronunciations_match_but_unrelated_words_do_not() {
        for (a, b) in [
            ("конфидай", "ConfidAI"),
            ("кодекс", "Codex"),
            ("докер", "Docker"),
            ("фигма", "Figma"),
        ] {
            assert!(close(a, b), "{a} -> {b}");
        }
        for (a, b) in [("встречу", "Codex"), ("код", "Codex"), ("завтра", "Docker")]
        {
            assert!(!close(a, b));
        }
    }
    #[test]
    fn candidates_name_exact_spans_and_keep_numbers_out() {
        let c = candidates(
            "Обсудим конфидай и кодекс в 15 часов.",
            &["ConfidAI".into(), "Codex".into()],
        );
        assert!(
            c.iter()
                .any(|c| c.heard == "конфидай" && c.term == "ConfidAI")
        );
        assert!(c.iter().any(|c| c.heard == "кодекс" && c.term == "Codex"));
        assert!(!c.iter().any(|c| c.heard.contains("15")));
    }
}
