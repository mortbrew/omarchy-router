//! Value coercion and display formatting.

use serde_json::Value;

use crate::metrics::Kind;

/// Metrics may arrive as numbers or strings. Accept both.
pub fn num(value: Option<&Value>) -> f64 {
    match value {
        None | Some(Value::Null) => 0.0,
        Some(Value::Bool(_)) => 0.0,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s
            .replace(',', "")
            .trim()
            .parse::<f64>()
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Analytics responses vary: a bare list, `{data: [...]}`, or `{data: {data: [...]}}`.
pub fn rows_of(payload: &Value) -> Vec<Value> {
    let mut current = payload.clone();
    for _ in 0..4 {
        match &current {
            Value::Array(rows) => return rows.clone(),
            Value::Object(map) => {
                let mut descended = false;
                for field in ["data", "results", "rows"] {
                    match map.get(field) {
                        Some(Value::Array(rows)) => return rows.clone(),
                        Some(inner @ Value::Object(_)) => {
                            current = inner.clone();
                            descended = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if !descended {
                    return Vec::new();
                }
            }
            _ => return Vec::new(),
        }
    }
    Vec::new()
}

/// Render a value according to its metric kind.
pub fn format(kind: Kind, value: f64) -> String {
    match kind {
        Kind::Money => {
            if value != 0.0 && value.abs() < 0.01 {
                format!("${value:.4}")
            } else {
                format!("${value:.2}")
            }
        }
        Kind::Int => int_grouped(value),
        Kind::Pct => format!("{:.1}%", value * 100.0),
        Kind::Ms => format!("{value:.0}ms"),
        Kind::Rate => format!("{value:.1}/s"),
    }
}

/// Money with thousands separators, for the key listing.
pub fn money_grouped(value: f64) -> String {
    let raw = if value != 0.0 && value.abs() < 0.01 {
        format!("{value:.4}")
    } else {
        format!("{value:.2}")
    };
    let (sign, digits) = match raw.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", raw.as_str()),
    };
    let (int_part, frac) = match digits.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (digits, None),
    };
    let mut grouped = String::new();
    for (idx, ch) in int_part.chars().enumerate() {
        if idx > 0 && (int_part.len() - idx) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    match frac {
        Some(f) => format!("{sign}${grouped}.{f}"),
        None => format!("{sign}${grouped}"),
    }
}

/// Integer with thousands separators.
pub fn int_grouped(value: f64) -> String {
    let n = value.round() as i64;
    let digits = n.abs().to_string();
    let mut grouped = String::new();
    for (idx, ch) in digits.chars().enumerate() {
        if idx > 0 && (digits.len() - idx) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    if n < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}
