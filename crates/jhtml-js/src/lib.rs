//! Stage 6: the JS sandbox (Boa, compute-only).
//!
//! Contract: user scripts run BEFORE layout in a bare Boa realm — no DOM,
//! no network, no filesystem, no timers, with a loop-iteration cap. The
//! only host surface is the `data` Map: scripts call `data.set(key, value)`
//! and the resulting entries are substituted into the document wherever
//! `{{key}}` placeholders appear. Scripts shape data, never layout.

use boa_engine::{js_string, Context, Source};
use std::collections::HashMap;

/// Result of running user scripts over the input document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptOutput {
    /// key -> stringified value from `data.set(key, value)`.
    pub data: HashMap<String, String>,
}

/// Errors from script execution (syntax, runtime, limit exceeded).
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptError {
    pub message: String,
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

fn strip_noise(msg: &str) -> String {
    // Boa error strings carry a source snippet; keep the first line.
    msg.lines().next().unwrap_or(msg).to_string()
}

const MAX_LOOP_ITERATIONS: u64 = 100_000;
const MAX_RECURSION: usize = 64;
const MAX_JSON_BYTES: usize = 1_000_000;

/// Run compute-only scripts in a sandboxed realm, returning their data.
pub fn run_scripts(scripts: &[String]) -> Result<ScriptOutput, ScriptError> {
    let mut out = ScriptOutput::default();
    if scripts.is_empty() {
        return Ok(out);
    }
    let mut context = Context::default();
    context
        .runtime_limits_mut()
        .set_loop_iteration_limit(MAX_LOOP_ITERATIONS);
    context
        .runtime_limits_mut()
        .set_recursion_limit(MAX_RECURSION);

    // Host data surface: a plain Map + JSON bridge.
    context
        .eval(Source::from_bytes(b"const data = new Map();"))
        .map_err(|e| ScriptError {
            message: strip_noise(&e.to_string()),
        })?;

    for script in scripts {
        context
            .eval(Source::from_bytes(script.as_bytes()))
            .map_err(|e| ScriptError {
                message: strip_noise(&e.to_string()),
            })?;
    }

    let json = context
        .eval(Source::from_bytes(
            b"JSON.stringify(Array.from(data.entries()))",
        ))
        .and_then(|v| v.to_string(&mut context).map(|s| s.to_std_string_escaped()))
        .map_err(|e| ScriptError {
            message: strip_noise(&e.to_string()),
        })?;
    if json.len() > MAX_JSON_BYTES {
        return Err(ScriptError {
            message: "data payload too large".into(),
        });
    }
    out.data = parse_entries(&json);
    Ok(out)
}

/// Parse `[["k","v"],...]` produced by JSON.stringify of Map entries.
fn parse_entries(json: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut chars = json.chars().peekable().enumerate();
    let tokens = split_top_level_arrays(json);
    for pair in tokens {
        if let Some((k, v)) = split_json_pair(&pair) {
            map.insert(unquote(k), unquote(v));
        }
    }
    let _ = chars;
    map
}

/// Split `[[a,b],[c,d]]` into `"[a,b]"` pieces at depth 1.
fn split_top_level_arrays(json: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    for (i, ch) in json.char_indices() {
        match ch {
            '[' => {
                depth += 1;
                if depth == 2 {
                    start = Some(i);
                }
            }
            ']' => {
                if depth == 2 {
                    if let Some(s) = start.take() {
                        out.push(json[s..=i].to_string());
                    }
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    out
}

/// Split `["k","v"]` into the two raw quoted strings.
fn split_json_pair(pair: &str) -> Option<(&str, &str)> {
    let inner = pair.strip_prefix('[')?.strip_suffix(']')?;
    let comma = find_top_level_comma(inner)?;
    Some((&inner[..comma], &inner[comma + 1..]))
}

fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut in_str = false;
    let mut escaped = false;
    for (i, ch) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_str => escaped = true,
            '"' => in_str = !in_str,
            ',' if !in_str => return Some(i),
            _ => {}
        }
    }
    None
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    let inner = t
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .unwrap_or(t);
    // Unescape the JSON escapes JSON.stringify produces for strings.
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Some(cp) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(cp);
                    }
                }
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Substitute `{{key}}` placeholders in a text node with script data.
pub fn substitute(text: &str, data: &HashMap<String, String>) -> String {
    if text.is_empty() || !text.contains("{{") || data.is_empty() {
        return text.to_string();
    }
    let mut out = text.to_string();
    for (k, v) in data {
        let placeholder = format!("{{{{{k}}}}}");
        if out.contains(&placeholder) {
            out = out.replace(&placeholder, v);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_and_inject() {
        let scripts = vec!["data.set('total', String(3 * 4 + 5));".to_string()];
        let out = run_scripts(&scripts).expect("ok");
        assert_eq!(out.data.get("total").map(String::as_str), Some("17"));
        assert_eq!(
            substitute("Total: ${{total}} USD", &out.data),
            "Total: $17 USD"
        );
    }

    #[test]
    fn loops_and_math_work() {
        let scripts = vec![
            "let s = 0; for (let i = 1; i <= 100; i++) s += i; data.set('sum', String(s));"
                .to_string(),
        ];
        let out = run_scripts(&scripts).unwrap();
        assert_eq!(out.data.get("sum").map(String::as_str), Some("5050"));
    }

    #[test]
    fn objects_stringify() {
        let scripts = vec!["data.set('item', JSON.stringify({sku: 'X1', qty: 3}));".to_string()];
        let out = run_scripts(&scripts).unwrap();
        assert_eq!(
            out.data.get("item").map(String::as_str),
            Some(r#"{"sku":"X1","qty":3}"#)
        );
    }

    #[test]
    fn syntax_error_reported() {
        let err = run_scripts(&vec!["data.set(".to_string()]).unwrap_err();
        assert!(!err.message.is_empty());
    }

    #[test]
    fn runtime_error_reported() {
        let err = run_scripts(&vec!["null.foo();".to_string()]).unwrap_err();
        assert!(!err.message.is_empty());
    }

    #[test]
    fn no_dom_no_net_no_fs() {
        // The realm is bare: these globals must not exist.
        let scripts = vec![
            "if (typeof document !== 'undefined') data.set('dom', 'yes');".to_string(),
            "if (typeof fetch !== 'undefined') data.set('net', 'yes');".to_string(),
            "if (typeof require !== 'undefined') data.set('fs', 'yes');".to_string(),
        ];
        let out = run_scripts(&scripts).unwrap();
        assert!(out.data.is_empty());
    }

    #[test]
    fn runaway_loop_is_capped() {
        // Would spin forever without the iteration limit; must error out.
        let scripts = vec!["let x = 0; while (true) { x = x + 1; }".to_string()];
        assert!(run_scripts(&scripts).is_err());
    }

    #[test]
    fn substitute_leaves_unknown_placeholders() {
        let data = HashMap::new();
        assert_eq!(substitute("Hi {{name}}", &data), "Hi {{name}}");
        assert_eq!(substitute("no placeholders", &data), "no placeholders");
    }

    #[test]
    fn json_escapes_unescaped() {
        let data = parse_entries(r#"[["msg","line1\nline2 \"q\""]]"#);
        assert_eq!(
            data.get("msg").map(String::as_str),
            Some("line1\nline2 \"q\"")
        );
    }
}
