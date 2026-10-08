//! Canonical (stable) JSON serialization for the conformance harness.
//!
//! Two runs must produce byte-identical output for the same logical value, so
//! the writer sorts object members by name, emits numbers in their normal form,
//! and never depends on hash-map iteration order. This is the text the harness
//! diffs against the C++ golden dump.

use serde_json::Value;

/// Write `value` into `out` in canonical form.
pub fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => out.push_str(&canonical_number(number)),
        Value::String(text) => write_canonical_string(text, out),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(members) => {
            let mut names: Vec<&str> = members.keys().map(String::as_str).collect();
            names.sort_unstable();
            out.push('{');
            for (index, name) in names.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical_string(name, out);
                out.push(':');
                // The key came from the same map, so the lookup cannot fail.
                if let Some(member) = members.get(name) {
                    write_canonical(member, out);
                }
            }
            out.push('}');
        }
    }
}

/// Serialize `value` to canonical text.
pub fn to_canonical_string(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn canonical_number(number: &serde_json::Number) -> String {
    // Floats compare by their shortest round-trip form; integers keep their
    // exact decimal digits so `1` and `1.0` are deliberately distinct.
    number.to_string()
}

fn write_canonical_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
