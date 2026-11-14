//! Conditions under which a setting applies, written as text so that profiles can use them too:
//!
//! - `gamescope`: the setting is on (`!gamescope`: off)
//! - `mouse=dpi`, `graphics_api=d3d11|d3d12`: one of these values (`!=`: none of them)
//! - `render_scale<100`, `frame_cap>0`: a number below or above
//! - commas join conditions that must all hold: `gamescope, render_scale<100`

use crate::value::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
enum Test {
    On,
    Off,
    In(Vec<String>),
    NotIn(Vec<String>),
    Below(f64),
    Above(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct When {
    clauses: Vec<(String, Test)>,
}

impl When {
    pub fn parse(text: &str) -> Result<When, String> {
        let mut clauses = Vec::new();
        for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (key, test) = if let Some((k, v)) = part.split_once("!=") {
                (k, Test::NotIn(list(v)))
            } else if let Some((k, v)) = part.split_once('=') {
                (k, Test::In(list(v)))
            } else if let Some((k, v)) = part.split_once('<') {
                (k, Test::Below(number(v, part)?))
            } else if let Some((k, v)) = part.split_once('>') {
                (k, Test::Above(number(v, part)?))
            } else if let Some(k) = part.strip_prefix('!') {
                (k, Test::Off)
            } else {
                (part, Test::On)
            };
            let key = key.trim();
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
                return Err(format!("“{part}”: expected a setting's name"));
            }
            clauses.push((key.to_string(), test));
        }
        Ok(When { clauses })
    }

    /// The settings this condition reads.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.clauses.iter().map(|(k, _)| k.as_str())
    }

    pub fn holds(&self, values: &BTreeMap<String, Value>) -> bool {
        self.clauses.iter().all(|(key, test)| {
            let v = values.get(key);
            match test {
                Test::On => v.is_some_and(Value::as_bool),
                Test::Off => !v.is_some_and(Value::as_bool),
                Test::In(list) => v.is_some_and(|v| list.iter().any(|t| matches(v, t))),
                Test::NotIn(list) => !v.is_some_and(|v| list.iter().any(|t| matches(v, t))),
                Test::Below(n) => v.and_then(Value::as_f64).is_some_and(|x| x < *n),
                Test::Above(n) => v.and_then(Value::as_f64).is_some_and(|x| x > *n),
            }
        })
    }
}

fn list(v: &str) -> Vec<String> {
    v.split('|').map(|s| s.trim().to_string()).collect()
}

fn number(v: &str, part: &str) -> Result<f64, String> {
    v.trim().parse().map_err(|_| format!("“{part}”: expected a number after the comparison"))
}

fn matches(v: &Value, text: &str) -> bool {
    match v {
        Value::Str(s) => s == text,
        Value::Bool(b) => *b == matches!(text, "1" | "true" | "on" | "yes"),
        _ => text.parse::<f64>().ok().zip(v.as_f64()).is_some_and(|(t, x)| (t - x).abs() < 1e-9),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn conditions() {
        let v = vals(&[("gamescope", true.into()), ("render_scale", 75.into()), ("mouse", "dpi".into())]);
        assert!(When::parse("gamescope").unwrap().holds(&v));
        assert!(!When::parse("!gamescope").unwrap().holds(&v));
        assert!(When::parse("gamescope, render_scale<100").unwrap().holds(&v));
        assert!(!When::parse("render_scale>80").unwrap().holds(&v));
        assert!(When::parse("mouse=dpi|gamescope").unwrap().holds(&v));
        assert!(!When::parse("mouse!=dpi").unwrap().holds(&v));
        assert!(When::parse("graphics_api!=d3d12").unwrap().holds(&v), "a missing setting is none of the values");
        assert!(When::parse("bad key").is_err());
    }
}
