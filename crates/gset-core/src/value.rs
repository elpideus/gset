//! A setting's value: what a TOML file can hold for one setting.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl Value {
    pub fn str(s: impl Into<String>) -> Self {
        Value::Str(s.into())
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => matches!(s.as_str(), "1" | "true" | "on" | "yes"),
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Bool(b) => Some(f64::from(u8::from(*b))),
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Str(s) => s.trim().parse().ok(),
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) if f.fract() == 0.0 => Some(*f as i64),
            Value::Str(s) => s.trim().parse().ok(),
            Value::Bool(b) => Some(i64::from(*b)),
            Value::Float(_) => None,
        }
    }

    /// The text of a string value; other kinds give an empty string.
    pub fn as_str(&self) -> &str {
        match self {
            Value::Str(s) => s,
            _ => "",
        }
    }

    /// Equal as settings: 1 and 1.0 are the same number, "75" and 75 too.
    pub fn same(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Bool(_), _) | (_, Value::Bool(_)) => self.as_bool() == other.as_bool(),
            _ => match (self.as_f64(), other.as_f64()) {
                (Some(a), Some(b)) => (a - b).abs() < 1e-9,
                _ => false,
            },
        }
    }

    pub fn to_toml(&self) -> toml_edit::Value {
        match self {
            Value::Bool(b) => (*b).into(),
            Value::Int(i) => (*i).into(),
            Value::Float(f) => (*f).into(),
            Value::Str(s) => s.as_str().into(),
        }
    }

    pub fn from_toml(item: &toml_edit::Item) -> Option<Value> {
        let v = item.as_value()?;
        Some(match v {
            toml_edit::Value::Boolean(b) => Value::Bool(*b.value()),
            toml_edit::Value::Integer(i) => Value::Int(*i.value()),
            toml_edit::Value::Float(f) => Value::Float(*f.value()),
            toml_edit::Value::String(s) => Value::Str(s.value().clone()),
            _ => return None,
        })
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.same(other)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{}", if *b { "on" } else { "off" }),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::Str(s) => write!(f, "{s}"),
        }
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}
impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Int(i)
    }
}
impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}
impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Str(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_compare_across_kinds() {
        assert!(Value::Int(75).same(&Value::Float(75.0)));
        assert!(Value::str("75").same(&Value::Int(75)));
        assert!(!Value::Int(75).same(&Value::Int(67)));
        assert!(Value::Bool(true).same(&Value::Int(1)));
        assert!(!Value::str("fsr").same(&Value::str("nis")));
    }
}
