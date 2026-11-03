use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

const MAX_APPS: usize = 64;
const MAX_TERMS: usize = 256;
const MAX_SNAPSHOTS: usize = 4;
const MAX_SNAPSHOT_CHARS: usize = 6000;
const MAX_APP_CHARS: usize = 12000;
const RETENTION_SECONDS: u64 = 30 * 24 * 60 * 60;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Dictionary {
    pub application: String,
    pub scope: String,
    pub terms: Vec<String>,
    pub updated_at: u64,
    #[serde(default)]
    pub term_seen: BTreeMap<String, u64>,
    #[serde(default)]
    pub texts: Vec<TextSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextSnapshot {
    pub text: String,
    pub seen_at: u64,
}

impl Dictionary {
    pub fn text(&self) -> String {
        let texts = fresh_texts(self, now());
        texts.iter().fold(String::new(), |text, item| {
            super::reference::combine(&text, &item.text)
        })
    }
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
            .filter(|b| b.len() <= 8 * 1024 * 1024)
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

fn fresh_texts(dictionary: &Dictionary, at: u64) -> Vec<TextSnapshot> {
    let mut remaining = MAX_APP_CHARS;
    dictionary
        .texts
        .iter()
        .filter(|item| at.saturating_sub(item.seen_at) <= RETENTION_SECONDS)
        .take(MAX_SNAPSHOTS)
        .filter_map(|item| {
            let text: String = item
                .text
                .chars()
                .take(remaining.min(MAX_SNAPSHOT_CHARS))
                .collect();
            remaining = remaining.saturating_sub(text.chars().count());
            (!text.is_empty()).then_some(TextSnapshot {
                text,
                seen_at: item.seen_at,
            })
        })
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
            d.texts = fresh_texts(&d, now());
            d
        })
        .filter(|d| !d.terms.is_empty() || !d.texts.is_empty())
        .collect()
}
pub fn text(scope: &str) -> String {
    store()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .apps
        .get(scope)
        .filter(|d| current(d))
        .map(Dictionary::text)
        .unwrap_or_default()
}
pub fn expire() -> Result<(), String> {
    let mut store = store().lock().unwrap_or_else(|e| e.into_inner());
    let mut changed = false;
    store.apps.retain(|_, dictionary| {
        let fresh = fresh_terms(dictionary, now());
        let texts = fresh_texts(dictionary, now());
        changed |= fresh.len() != dictionary.terms.len()
            || texts.len() != dictionary.texts.len()
            || !current(dictionary);
        dictionary.terms = fresh;
        dictionary.texts = texts;
        dictionary
            .term_seen
            .retain(|term, _| dictionary.terms.contains(term));
        current(dictionary) && (!dictionary.terms.is_empty() || !dictionary.texts.is_empty())
    });
    if changed {
        save(&store)?;
    }
    Ok(())
}
pub fn learn(scope: &str, application: &str, screen: &str, generation: u64) -> Result<(), String> {
    let fresh: String = super::reference::combine(screen, "")
        .chars()
        .take(MAX_SNAPSHOT_CHARS)
        .collect();
    if fresh.is_empty() {
        return Ok(());
    }
    let mut store = store().lock().unwrap_or_else(|e| e.into_inner());
    if store.generation != generation {
        return Ok(());
    }
    store.apps.retain(|_, d| current(d));
    let at = now();
    let mut dictionary = store.apps.get(scope).cloned().unwrap_or_default();
    for term in &dictionary.terms {
        dictionary
            .term_seen
            .entry(term.clone())
            .or_insert(dictionary.updated_at);
    }
    let mut texts = vec![TextSnapshot {
        text: fresh.clone(),
        seen_at: at,
    }];
    if let Some(previous) = store.apps.get(scope) {
        for snapshot in fresh_texts(previous, at) {
            if snapshot.text != fresh {
                texts.push(snapshot);
            }
        }
    }
    dictionary.application = application.into();
    dictionary.scope = scope.into();
    dictionary.updated_at = at;
    dictionary.texts = texts;
    dictionary.texts = fresh_texts(&dictionary, at);
    store.version = 2;
    store.apps.insert(scope.into(), dictionary);
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plain_text_memory_expires_per_snapshot_and_replaces_legacy_keyword_context() {
        let at = now();
        let mut dictionary = Dictionary {
            terms: vec!["LegacyNoise".into()],
            updated_at: at,
            texts: vec![
                TextSnapshot {
                    text: "SLP: план встречи и отчёты\nSLP сообщество".into(),
                    seen_at: at,
                },
                TextSnapshot {
                    text: "ExpiredContext".into(),
                    seen_at: at - RETENTION_SECONDS - 1,
                },
            ],
            ..Default::default()
        };
        assert_eq!(fresh_texts(&dictionary, at).len(), 1);
        let text = dictionary.text();
        assert!(text.contains("SLP сообщество"));
        assert!(!text.contains("LegacyNoise"));
        assert!(!text.contains("ExpiredContext"));
        dictionary.texts.clear();
        assert!(
            dictionary.text().is_empty(),
            "keyword-only data is not reused as prose"
        );
        assert_eq!(
            dictionary.terms,
            vec!["LegacyNoise"],
            "old data is preserved"
        );
    }

    #[test]
    fn remembered_text_is_bounded_and_never_combined_between_apps() {
        let at = now();
        let telegram = Dictionary {
            updated_at: at,
            texts: (0..10)
                .map(|_| TextSnapshot {
                    text: "я".repeat(20_000),
                    seen_at: at,
                })
                .collect(),
            ..Default::default()
        };
        let snapshots = fresh_texts(&telegram, at);
        assert!(snapshots.len() <= MAX_SNAPSHOTS);
        assert!(
            snapshots
                .iter()
                .map(|s| s.text.chars().count())
                .sum::<usize>()
                <= MAX_APP_CHARS
        );
        let editor = Dictionary {
            updated_at: at,
            texts: vec![TextSnapshot {
                text: "Editor only".into(),
                seen_at: at,
            }],
            ..Default::default()
        };
        assert_eq!(editor.text(), "Editor only");
        assert!(!telegram.text().contains("Editor"));
    }
    #[test]
    fn terms_expire_individually_even_if_an_app_keeps_being_used() {
        let at = RETENTION_SECONDS + 100;
        let d = Dictionary {
            application: "Telegram".into(),
            scope: "test.telegram".into(),
            terms: vec!["Old".into(), "Fresh".into()],
            updated_at: at,
            term_seen: BTreeMap::from([("Old".into(), 1), ("Fresh".into(), at)]),
            texts: vec![],
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
