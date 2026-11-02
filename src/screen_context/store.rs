use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

const MAX_APPS: usize = 64;
const MAX_TERMS: usize = 256;
const RETENTION_SECONDS: u64 = 30 * 24 * 60 * 60;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Dictionary {
    pub application: String,
    pub scope: String,
    pub terms: Vec<String>,
    pub updated_at: u64,
    #[serde(default)]
    pub term_seen: BTreeMap<String, u64>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct Store {
    version: u32,
    #[serde(skip)]
    generation: u64,
    apps: BTreeMap<String, Dictionary>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn path() -> Result<std::path::PathBuf, String> {
    Ok(crate::app_paths::support_dir()
        .map_err(|e| e.to_string())?
        .join("app-vocabulary.json"))
}
fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| {
        let saved = path()
            .ok()
            .and_then(|p| std::fs::read(p).ok())
            .filter(|b| b.len() <= 2 * 1024 * 1024)
            .and_then(|b| serde_json::from_slice::<Store>(&b).ok())
            .unwrap_or_default();
        Mutex::new(saved)
    })
}
fn current(d: &Dictionary) -> bool {
    now().saturating_sub(d.updated_at) <= RETENTION_SECONDS
}

fn fresh_terms(dictionary: &Dictionary, at: u64) -> Vec<String> {
    dictionary
        .terms
        .iter()
        .filter(|term| {
            let seen = dictionary
                .term_seen
                .get(*term)
                .copied()
                .unwrap_or(dictionary.updated_at);
            at.saturating_sub(seen) <= RETENTION_SECONDS
        })
        .take(MAX_TERMS)
        .cloned()
        .collect()
}
pub fn generation() -> u64 {
    store().lock().unwrap_or_else(|e| e.into_inner()).generation
}
pub fn dictionaries() -> Vec<Dictionary> {
    store()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .apps
        .values()
        .filter(|d| current(d))
        .map(|d| {
            let mut d = d.clone();
            d.terms = fresh_terms(&d, now());
            d
        })
        .filter(|d| !d.terms.is_empty())
        .collect()
}
pub fn terms(scope: &str) -> Vec<String> {
    store()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .apps
        .get(scope)
        .filter(|d| current(d))
        .map(|d| fresh_terms(d, now()))
        .unwrap_or_default()
}
pub fn expire() -> Result<(), String> {
    let mut store = store().lock().unwrap_or_else(|e| e.into_inner());
    let mut changed = false;
    store.apps.retain(|_, dictionary| {
        let fresh = fresh_terms(dictionary, now());
        changed |= fresh.len() != dictionary.terms.len() || !current(dictionary);
        dictionary.terms = fresh;
        dictionary
            .term_seen
            .retain(|term, _| dictionary.terms.contains(term));
        current(dictionary) && !dictionary.terms.is_empty()
    });
    if changed {
        save(&store)?;
    }
    Ok(())
}
pub fn learn(scope: &str, application: &str, screen: &str, generation: u64) -> Result<(), String> {
    let fresh = extract_terms(screen);
    if fresh.is_empty() {
        return Ok(());
    }
    let mut store = store().lock().unwrap_or_else(|e| e.into_inner());
    if store.generation != generation {
        return Ok(());
    }
    store.apps.retain(|_, d| current(d));
    let at = now();
    let mut seen: BTreeMap<String, u64> = fresh.iter().map(|t| (t.clone(), at)).collect();
    let mut terms = fresh;
    if let Some(previous) = store.apps.get(scope) {
        for term in &fresh_terms(previous, at) {
            if !terms
                .iter()
                .any(|t| t.to_lowercase() == term.to_lowercase())
            {
                seen.insert(
                    term.clone(),
                    previous
                        .term_seen
                        .get(term)
                        .copied()
                        .unwrap_or(previous.updated_at),
                );
                terms.push(term.clone());
            }
        }
    }
    terms.truncate(MAX_TERMS);
    store.version = 1;
    store.apps.insert(
        scope.into(),
        Dictionary {
            application: application.into(),
            scope: scope.into(),
            terms,
            updated_at: at,
            term_seen: seen,
        },
    );
    while store.apps.len() > MAX_APPS {
        if let Some(oldest) = store
            .apps
            .values()
            .min_by_key(|d| d.updated_at)
            .map(|d| d.scope.clone())
        {
            store.apps.remove(&oldest);
        }
    }
    save(&store)
}
pub fn forget(scope: &str) -> Result<(), String> {
    let mut store = store().lock().unwrap_or_else(|e| e.into_inner());
    if scope == "all" {
        store.apps.clear();
    } else {
        store.apps.remove(scope);
    }
    store.generation = store.generation.wrapping_add(1);
    save(&store)
}
fn save(store: &Store) -> Result<(), String> {
    let path = path()?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    private_write(
        &path,
        &serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?,
    )
}
pub(super) fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    // Set permissions before writing any terms, including on replacement.
    use std::io::Write;
    let temporary = path.with_extension("private-tmp");
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, path).map_err(|e| e.to_string())
}

/// Keep vocabulary rather than conversations: names, technical words, and
/// identifiers. Exclude URLs, addresses, long numeric strings and common UI.
pub(super) fn extract_terms(screen: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "the",
        "and",
        "with",
        "this",
        "that",
        "from",
        "your",
        "have",
        "will",
        "file",
        "edit",
        "view",
        "help",
        "window",
        "search",
        "message",
        "settings",
        "telegram",
        "today",
        "yesterday",
        "send",
        "online",
        "для",
        "что",
        "это",
        "как",
        "все",
        "так",
        "или",
        "при",
        "его",
        "она",
        "они",
        "мне",
        "тебя",
        "привет",
        "сегодня",
        "вчера",
        "сообщение",
        "настройки",
        "поиск",
        "отправить",
        "изменить",
        "файл",
        "правка",
        "окно",
        "справка",
        "давай",
        "нужно",
        "можно",
    ];
    let mut seen = HashSet::new();
    let mut scored = Vec::new();
    for token in screen.split_whitespace().take(5000) {
        if token.contains(['@', '/', '\\']) || token.contains("www.") {
            continue;
        }
        let term = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');
        let n = term.chars().count();
        let lower = term.to_lowercase();
        if !(3..=48).contains(&n) || STOP.contains(&lower.as_str()) || !seen.insert(lower) {
            continue;
        }
        if term.chars().filter(|c| c.is_numeric()).count() > 3
            || term.chars().filter(|c| c.is_alphabetic()).count() < 2
            || term.contains('.')
        {
            continue;
        }
        let capitals = term.chars().filter(|c| c.is_uppercase()).count();
        let latin = term.chars().filter(|c| c.is_ascii_alphabetic()).count();
        let score = if capitals >= 2 {
            4
        } else if latin > 0 && capitals > 0 {
            3
        } else if capitals > 0 && n >= 4 {
            2
        } else if latin >= 4 || n >= 7 {
            1
        } else {
            continue;
        };
        scored.push((score, term.to_owned()));
    }
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, t)| t).take(96).collect()
}

/// Exact whole-word case restoration is safe even without an LLM.
pub(super) fn exact_spellings(text: &str, terms: &[String]) -> String {
    let pattern = regex::Regex::new(r"[\p{L}\p{N}_-]+").unwrap();
    pattern
        .replace_all(text, |caps: &regex::Captures<'_>| {
            let heard = &caps[0];
            terms
                .iter()
                .find(|t| t.to_lowercase() == heard.to_lowercase())
                .cloned()
                .unwrap_or_else(|| heard.into())
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terms_expire_individually_even_if_an_app_keeps_being_used() {
        let at = RETENTION_SECONDS + 100;
        let d = Dictionary {
            application: "Telegram".into(),
            scope: "test.telegram".into(),
            terms: vec!["Old".into(), "Fresh".into()],
            updated_at: at,
            term_seen: BTreeMap::from([("Old".into(), 1), ("Fresh".into(), at)]),
        };
        assert_eq!(fresh_terms(&d, at), vec!["Fresh"]);
        let apps = BTreeMap::from([
            ("test.telegram", d),
            (
                "test.editor",
                Dictionary {
                    terms: vec!["EditorOnly".into()],
                    updated_at: at,
                    ..Default::default()
                },
            ),
        ]);
        assert_eq!(fresh_terms(&apps["test.editor"], at), vec!["EditorOnly"]);
    }
    #[test]
    #[cfg(unix)]
    fn dictionary_replacements_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let directory =
            std::env::temp_dir().join(format!("volna-private-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let file = directory.join("dictionary.json");
        private_write(&file, b"test").unwrap();
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        private_write(&file, b"replacement").unwrap();
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
