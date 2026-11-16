//! The RDF-derived dictionaries (RIF and wiki), reproducing the
//! indexer's `build_rdf_cache(..., remove_common_words=True)`.
//!
//! The indexer fetches this text over SPARQL; the lane replays it from
//! the authoritative cache file written during the reference build
//! (`~/spikes/index-self-build/rdf-cache.json`, produced by the
//! batched patched script — the cache format is defined by
//! `index-genenetwork-batched`, not by live SPARQL).
//!
//! Each cache is treated as ONE monolithic hashed input: a change to
//! either dictionary re-derives every gene document (the design's
//! "cache as one input", not per-symbol attribution).

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use super::sha256::hex_digest;

/// Species batching order, verbatim from `index-genenetwork-batched`
/// (`ALL_SPECIES`). Dictionary values are keyed on
/// `(speciesName, symbolName)` and the word counter's first-seen order
/// follows this order.
pub const ALL_SPECIES: [&str; 14] = [
    "arabidopsis",
    "barley",
    "bat",
    "drosophila",
    "human",
    "medaka",
    "monkey",
    "mouse",
    "poplar",
    "rat",
    "soybean",
    "tomato",
    "macaca",
    "Nematode",
];

/// One derived dictionary plus its monolithic content hash.
pub struct Dictionary {
    entries: HashMap<(String, String), String>,
    pub hash: String,
}

impl Dictionary {
    /// The dictionary text for `(species, symbol)`, if any.
    pub fn get(&self, species: &str, symbol: &str) -> Option<&str> {
        self.entries
            .get(&(species.to_string(), symbol.to_string()))
            .map(String::as_str)
    }
}

/// Both RDF dictionaries (RIF, wiki) plus their content hashes.
pub struct RdfCaches {
    pub rif: Dictionary,
    pub wiki: Dictionary,
}

impl RdfCaches {
    /// Load and derive both dictionaries from the cache file.
    pub fn load(path: &Path) -> Result<RdfCaches, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read RDF cache {}: {e}", path.display()))?;
        let store: Value = serde_json::from_str(&text)
            .map_err(|e| format!("RDF cache {} is not valid JSON: {e}", path.display()))?;
        Ok(RdfCaches {
            rif: derive(&store, "rif")?,
            wiki: derive(&store, "wiki")?,
        })
    }
}

/// Indexer word tokenisation: Python `re.compile(r"\w+")` with a
/// `str` subject (Unicode word characters: alphanumerics plus `_`).
/// Callers join the runs with single spaces.
fn tokenize_words(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;
    for (index, ch) in text.char_indices() {
        let is_word = ch.is_alphanumeric() || ch == '_';
        match (is_word, start) {
            (true, None) => start = Some(index),
            (false, Some(begin)) => {
                tokens.push(&text[begin..index]);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(begin) = start {
        tokens.push(&text[begin..]);
    }
    tokens
}

/// Derive one dictionary from the parsed cache store.
fn derive(store: &Value, kind: &str) -> Result<Dictionary, String> {
    let mut cache: HashMap<(String, String), String> = HashMap::new();
    // Word counts with first-seen insertion order (Python Counter).
    let mut counts: HashMap<String, u64> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let kind_store = store
        .get(kind)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("RDF cache has no `{kind}` section"))?;
    for species in ALL_SPECIES {
        let Some(bindings) = kind_store.get(species).and_then(Value::as_array) else {
            continue;
        };
        for binding in bindings {
            let species_name = binding
                .get("speciesName")
                .and_then(|v| v.get("value"))
                .and_then(Value::as_str)
                .ok_or("binding missing speciesName.value")?;
            let symbol_name = binding
                .get("symbolName")
                .and_then(|v| v.get("value"))
                .and_then(Value::as_str)
                .ok_or("binding missing symbolName.value")?;
            let comment = binding
                .get("comment")
                .and_then(|v| v.get("value"))
                .and_then(Value::as_str)
                .ok_or("binding missing comment.value")?;
            let value = tokenize_words(comment).join(" ");
            cache.insert(
                (species_name.to_string(), symbol_name.to_string()),
                value.clone(),
            );
            // Counter.update(value.lower().strip().split())
            for word in value.to_lowercase().split_whitespace() {
                match counts.get_mut(word) {
                    Some(count) => *count += 1,
                    None => {
                        counts.insert(word.to_string(), 1);
                        order.push(word.to_string());
                    }
                }
            }
        }
    }

    // remove_common_words: top-1000 by count (ties by first-seen order),
    // then drop words with len<4 or cnt>3000.
    let mut items: Vec<(String, u64)> = order
        .iter()
        .map(|word| (word.clone(), counts[word]))
        .collect();
    // Stable sort by count descending == heapq.nlargest(1000, ...) set
    // (verified against CPython on the authoritative cache; see
    // logs/cache-nlargest-check.log).
    items.sort_by(|a, b| b.1.cmp(&a.1));
    let mut dropped: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (word, count) in items.iter().take(1000) {
        if word.chars().count() < 4 || *count > 3000 {
            dropped.insert(word);
        }
    }

    let mut entries: HashMap<(String, String), String> = HashMap::new();
    for (key, value) in cache {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut words: Vec<&str> = Vec::new();
        let lowered = value.to_lowercase();
        for word in lowered.split_whitespace() {
            if dropped.contains(word) || !seen.insert(word) {
                continue;
            }
            words.push(word);
        }
        entries.insert(key, words.join(" "));
    }

    let hash = content_hash(&entries);
    Ok(Dictionary { entries, hash })
}

/// Monolithic content hash: a canonical, order-independent
/// serialization of the derived dictionary.
fn content_hash(entries: &HashMap<(String, String), String>) -> String {
    let mut keys: Vec<&(String, String)> = entries.keys().collect();
    keys.sort();
    let mut buffer: Vec<u8> = Vec::new();
    for key in keys {
        buffer.extend_from_slice(key.0.as_bytes());
        buffer.push(0);
        buffer.extend_from_slice(key.1.as_bytes());
        buffer.push(0);
        buffer.extend_from_slice(entries[key].as_bytes());
        buffer.push(1);
    }
    hex_digest(&buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punctuation_strip_joins_word_runs() {
        assert_eq!(tokenize_words("hello, world!").join(" "), "hello world");
        assert_eq!(
            tokenize_words("beta-1,4-galactosyl").join(" "),
            "beta 1 4 galactosyl"
        );
        assert_eq!(tokenize_words("a_b c").join(" "), "a_b c");
    }

    #[test]
    fn remove_common_words_matches_reference_slice() {
        // Mirrors derive-dictionary.py for (mouse, Cd3g):
        // rif='identifies cd3gamma infiltrating animals tcr graft cd3cd4'.
        let json = std::fs::read_to_string("/home/kabui/spikes/index-self-build/rdf-cache.json");
        let Ok(json) = json else {
            eprintln!("skipping: authoritative RDF cache not present");
            return;
        };
        let store: Value = serde_json::from_str(&json).unwrap();
        let rif = derive(&store, "rif").unwrap();
        // The dictionary text is fed to `index_text_without_positions`
        // (positions off, wdf increment 0), so only its word SET
        // reaches the index; Python's `" ".join(set(...))` order is
        // irrelevant and deliberately not reproduced.
        let mut got: Vec<&str> = rif
            .get("mouse", "Cd3g")
            .expect("mouse|Cd3g present")
            .split_whitespace()
            .collect();
        got.sort_unstable();
        let mut want: Vec<&str> = "identifies cd3gamma infiltrating animals tcr graft cd3cd4"
            .split_whitespace()
            .collect();
        want.sort_unstable();
        assert_eq!(got, want);
    }
}
