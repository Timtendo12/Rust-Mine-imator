//! Translations (`.milanguage`; `language_load`, `text_get`,
//! `minecraft_asset_get_name`).
//!
//! A language file is nested JSON. Keys that contain a `/` are groups whose
//! name (without the slash) is prepended to everything inside, so
//! `"type/": { "char": "Character" }` defines the text `typechar`.

use crate::json::{self, Json, JsonObject};
use crate::FormatError;
use std::collections::HashMap;

/// A set of texts by key.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Language {
    texts: HashMap<String, String>,
}

fn flatten(prefix: &str, map: &JsonObject, out: &mut HashMap<String, String>) {
    for (key, value) in map.iter() {
        if key.contains('/') {
            if let Json::Object(group) = value {
                flatten(&format!("{prefix}{}", key.replacen('/', "", 1)), group, out);
            }
        } else if let Json::String(text) = value {
            out.insert(format!("{prefix}{key}"), text.clone());
        }
    }
}

/// `string_format_snakecase`: `oak_stairs` becomes `Oak stairs`.
fn format_snake_case(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect::<String>().replace('_', " "),
        None => String::new(),
    }
}

impl Language {
    pub fn load(bytes: &[u8]) -> Result<Self, FormatError> {
        let root = json::parse(bytes)?;
        let map = root.as_object().ok_or_else(|| FormatError::Corrupted("the root is not an object".to_owned()))?;
        let mut texts = HashMap::new();
        flatten("", map, &mut texts);
        Ok(Self { texts })
    }

    pub fn len(&self) -> usize {
        self.texts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.texts.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.texts.get(key).map(String::as_str)
    }

    /// The text for `key` with `%1`, `%2`, ... replaced by `args`. A missing
    /// key gives a visible marker, as in the original.
    pub fn text(&self, key: &str, args: &[&str]) -> String {
        let Some(text) = self.get(key) else {
            return format!("<No text found for \"{key}\">");
        };
        let mut text = text.to_owned();
        for (i, arg) in args.iter().enumerate() {
            text = text.replacen(&format!("%{}", i + 1), arg, 1);
        }
        text
    }

    /// Display name of a Minecraft asset (`kind` is `model`, `modelpart`,
    /// `block`, ...). Assets without a translation show their identifier in
    /// readable form.
    pub fn asset_name(&self, kind: &str, name: &str) -> String {
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
            return name.to_owned();
        }
        match self.get(&format!("{kind}{name}")) {
            Some(text) => text.to_owned(),
            None => format_snake_case(name),
        }
    }

    /// Texts of `self`, falling back to `english` for missing keys.
    pub fn with_fallback(mut self, english: &Language) -> Language {
        for (key, text) in &english.texts {
            self.texts.entry(key.clone()).or_insert_with(|| text.clone());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = br#"{
        "description": "ignored? no, a plain key",
        "type/": { "char": "Character", "folder": "Folder" },
        "particle/": { "rain": "Rain", "type/": { "spark": "Spark" } },
        "recentlastopeneddate": "Last opened %1.%2.%3",
        "model/": { "human": "Human" },
        "number": 5
    }"#;

    #[test]
    fn groups_are_flattened() {
        let lang = Language::load(SAMPLE).unwrap();
        assert_eq!(lang.get("typechar"), Some("Character"));
        assert_eq!(lang.get("particlerain"), Some("Rain"));
        assert_eq!(lang.get("particletypespark"), Some("Spark"));
        assert_eq!(lang.get("description"), Some("ignored? no, a plain key"));
        assert_eq!(lang.get("number"), None);
        assert_eq!(lang.len(), 7);
    }

    #[test]
    fn arguments_and_missing_keys() {
        let lang = Language::load(SAMPLE).unwrap();
        assert_eq!(lang.text("recentlastopeneddate", &["14", "9", "2026"]), "Last opened 14.9.2026");
        assert_eq!(lang.text("nope", &[]), "<No text found for \"nope\">");
    }

    #[test]
    fn asset_names() {
        let lang = Language::load(SAMPLE).unwrap();
        assert_eq!(lang.asset_name("model", "human"), "Human");
        assert_eq!(lang.asset_name("block", "oak_stairs"), "Oak stairs");
        assert_eq!(lang.asset_name("block", "12"), "12");
        assert_eq!(lang.asset_name("block", ""), "");
    }

    #[test]
    fn fallback_fills_gaps() {
        let english = Language::load(SAMPLE).unwrap();
        let other = Language::load(br#"{ "type/": { "char": "Personnage" } }"#).unwrap().with_fallback(&english);
        assert_eq!(other.get("typechar"), Some("Personnage"));
        assert_eq!(other.get("typefolder"), Some("Folder"));
    }
}
