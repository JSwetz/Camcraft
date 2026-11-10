//! Key order of a `.mch` file (issue #480): the editor writes a saved
//! machine with its keys in the order the file was read, new keys
//! appended at the end of their object.
//!
//! Provenance: our own writer, specced from the issue's "Done means"
//! and the JSON grammar (RFC 8259); the pretty layout is the one
//! `serde_json::to_string_pretty` writes (two-space indent, `": "`).
//!
//! The workspace `serde_json` keeps object keys sorted (no
//! `preserve_order`, which would change every JSON writer, project
//! files included). Rather than a new dependency, the order is recorded
//! beside the value: one pass over the source text with a visitor that
//! notes each object's keys as they come, then a writer that walks the
//! value in that order.

use std::fmt;

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};

/// The order keys were read in: per object its keys (each with the
/// order below it), per array its items. Scalars carry nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyOrder {
    keys: Vec<(String, KeyOrder)>,
    items: Vec<KeyOrder>,
}

impl KeyOrder {
    /// The order of `text`'s keys; empty (= sorted output) when the
    /// text is not readable JSON.
    #[must_use]
    pub fn of_text(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    fn key(&self, k: &str) -> Option<&KeyOrder> {
        self.keys.iter().find(|(n, _)| n == k).map(|(_, o)| o)
    }
}

struct OrderVisitor;

impl<'de> Visitor<'de> for OrderVisitor {
    type Value = KeyOrder;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<KeyOrder, E> {
        Ok(KeyOrder::default())
    }
    fn visit_i64<E>(self, _: i64) -> Result<KeyOrder, E> {
        Ok(KeyOrder::default())
    }
    fn visit_u64<E>(self, _: u64) -> Result<KeyOrder, E> {
        Ok(KeyOrder::default())
    }
    fn visit_f64<E>(self, _: f64) -> Result<KeyOrder, E> {
        Ok(KeyOrder::default())
    }
    fn visit_str<E>(self, _: &str) -> Result<KeyOrder, E> {
        Ok(KeyOrder::default())
    }
    fn visit_unit<E>(self) -> Result<KeyOrder, E> {
        Ok(KeyOrder::default())
    }
    fn visit_none<E>(self) -> Result<KeyOrder, E> {
        Ok(KeyOrder::default())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<KeyOrder, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element::<KeyOrder>()? {
            items.push(item);
        }
        Ok(KeyOrder { keys: Vec::new(), items })
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<KeyOrder, A::Error> {
        let mut keys: Vec<(String, KeyOrder)> = Vec::new();
        while let Some(k) = map.next_key::<String>()? {
            let sub = map.next_value::<KeyOrder>()?;
            // A repeated key keeps its first place; its last value wins,
            // as it does in `serde_json::Value`.
            match keys.iter_mut().find(|(n, _)| *n == k) {
                Some(slot) => slot.1 = sub,
                None => keys.push((k, sub)),
            }
        }
        Ok(KeyOrder { keys, items: Vec::new() })
    }
}

impl<'de> Deserialize<'de> for KeyOrder {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(OrderVisitor)
    }
}

/// `value` as pretty JSON, each object's keys in `order` (keys the
/// order does not know follow, sorted). No trailing newline.
#[must_use]
pub fn to_pretty(value: &Value, order: &KeyOrder) -> String {
    let mut out = String::new();
    write(&mut out, value, order, 0);
    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn ordered<'a>(map: &'a Map<String, Value>, order: &KeyOrder) -> Vec<&'a String> {
    let mut keys: Vec<&String> = order.keys.iter().filter_map(|(k, _)| map.get_key_value(k).map(|(k, _)| k)).collect();
    keys.extend(map.keys().filter(|k| order.key(k).is_none()));
    keys
}

fn write(out: &mut String, value: &Value, order: &KeyOrder, depth: usize) {
    let empty = KeyOrder::default();
    match value {
        Value::Object(map) if !map.is_empty() => {
            out.push_str("{\n");
            for (i, k) in ordered(map, order).into_iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                indent(out, depth + 1);
                out.push_str(&serde_json::to_string(k).unwrap_or_else(|_| "\"\"".to_owned()));
                out.push_str(": ");
                if let Some(v) = map.get(k) {
                    write(out, v, order.key(k).unwrap_or(&empty), depth + 1);
                }
            }
            out.push('\n');
            indent(out, depth);
            out.push('}');
        }
        Value::Array(items) if !items.is_empty() => {
            out.push_str("[\n");
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                indent(out, depth + 1);
                write(out, v, order.items.get(i).unwrap_or(&empty), depth + 1);
            }
            out.push('\n');
            indent(out, depth);
            out.push(']');
        }
        // Scalars and empty containers: serde_json's own compact text
        // (`{}`, `[]`), the same the pretty writer emits.
        other => out.push_str(&serde_json::to_string(other).unwrap_or_else(|_| "null".to_owned())),
    }
}
