//! Blueprint strings: a version character (`0`), then base64 of zlib-compressed JSON.

use std::io::{Read, Write};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

/// Why a blueprint file yields no blueprints.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum BlueprintError {
    #[error("not a blueprint string: {0}")]
    Undecodable(String),
    #[error("holds a {0}, not a blueprint or blueprint book")]
    Unsupported(String),
    #[error("book entry {position} is a {kind}, not a blueprint")]
    BookEntry { position: usize, kind: String },
    #[error("the book holds no blueprints")]
    EmptyBook,
}

/// One blueprint to stamp, with the name its output save gets (before the user's prefix).
#[derive(Debug, Clone, PartialEq)]
pub struct Blueprint {
    pub name: String,
    pub string: String,
}

/// The JSON inside a blueprint string; surrounding whitespace is ignored.
pub fn decode(string: &str) -> Result<Value, BlueprintError> {
    let undecodable = |message: String| BlueprintError::Undecodable(message);
    let payload = string
        .trim()
        .strip_prefix('0')
        .ok_or_else(|| undecodable("does not start with version 0".into()))?;
    let compressed = STANDARD
        .decode(payload)
        .map_err(|error| undecodable(error.to_string()))?;
    let mut json = String::new();
    flate2::read::ZlibDecoder::new(compressed.as_slice())
        .read_to_string(&mut json)
        .map_err(|error| undecodable(error.to_string()))?;
    serde_json::from_str(&json).map_err(|error| undecodable(error.to_string()))
}

/// A version-0 blueprint string holding `value`, the inverse of [`decode`].
pub fn encode(value: &Value) -> String {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    encoder
        .write_all(value.to_string().as_bytes())
        .expect("writing to memory");
    format!(
        "0{}",
        STANDARD.encode(encoder.finish().expect("writing to memory"))
    )
}

/// The blueprints in a blueprint file. A single blueprint is named `stem`. A book yields one
/// blueprint per entry in the book's order (by `index`), named `stem_n` with `n` from 1.
pub fn blueprints(stem: &str, text: &str) -> Result<Vec<Blueprint>, BlueprintError> {
    let value = decode(text)?;
    let Some(object) = value.as_object() else {
        return Err(BlueprintError::Unsupported("non-object".into()));
    };
    if object.contains_key("blueprint") {
        return Ok(vec![Blueprint {
            name: stem.into(),
            string: text.trim().into(),
        }]);
    }
    let Some(book) = object.get("blueprint_book") else {
        return Err(BlueprintError::Unsupported(
            object.keys().next().cloned().unwrap_or_default(),
        ));
    };
    let mut entries: Vec<&Value> = book["blueprints"]
        .as_array()
        .map(|entries| entries.iter().collect())
        .unwrap_or_default();
    entries.sort_by_key(|entry| entry["index"].as_u64().unwrap_or(0));
    if entries.is_empty() {
        return Err(BlueprintError::EmptyBook);
    }
    entries
        .into_iter()
        .enumerate()
        .map(|(position, entry)| {
            let Some(blueprint) = entry.get("blueprint") else {
                let kind = entry
                    .as_object()
                    .and_then(|entry| entry.keys().find(|key| *key != "index"))
                    .cloned()
                    .unwrap_or_default();
                return Err(BlueprintError::BookEntry {
                    position: position + 1,
                    kind,
                });
            };
            Ok(Blueprint {
                name: format!("{stem}_{}", position + 1),
                string: encode(&json!({ "blueprint": blueprint })),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(entries: Value) -> String {
        encode(&json!({ "blueprint_book": { "blueprints": entries, "item": "blueprint-book" } }))
    }

    #[test]
    fn a_single_blueprint_is_named_after_the_file_stem() {
        let string = encode(&json!({ "blueprint": { "label": "x" } }));

        assert_eq!(
            blueprints("smelting", &string),
            Ok(vec![Blueprint {
                name: "smelting".into(),
                string
            }])
        );
    }

    #[test]
    fn book_blueprints_are_numbered_from_one_in_index_order() {
        let string = book(json!([
            { "index": 5, "blueprint": { "label": "second" } },
            { "index": 0, "blueprint": { "label": "first" } },
        ]));

        let found = blueprints("mall", &string).unwrap();

        let labels: Vec<(String, Value)> = found
            .iter()
            .map(|blueprint| {
                (
                    blueprint.name.clone(),
                    decode(&blueprint.string).unwrap()["blueprint"]["label"].clone(),
                )
            })
            .collect();
        assert_eq!(
            labels,
            vec![
                ("mall_1".into(), json!("first")),
                ("mall_2".into(), json!("second"))
            ]
        );
    }

    #[test]
    fn a_book_entry_that_is_not_a_blueprint_is_an_error() {
        let string = book(json!([{ "index": 0, "deconstruction_planner": {} }]));

        assert_eq!(
            blueprints("mall", &string),
            Err(BlueprintError::BookEntry {
                position: 1,
                kind: "deconstruction_planner".into()
            })
        );
    }

    #[test]
    fn a_string_that_is_not_a_blueprint_is_an_error() {
        assert!(matches!(
            blueprints("x", "hello"),
            Err(BlueprintError::Undecodable(_))
        ));
    }
}
