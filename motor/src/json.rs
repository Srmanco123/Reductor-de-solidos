//! Utilidades mínimas de JSON (solo escritura) y de URL.

use std::collections::HashMap;

/// Cadena JSON con comillas y escapes.
pub fn s(x: &str) -> String {
    let mut o = String::with_capacity(x.len() + 2);
    o.push('"');
    for c in x.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Número como lo escribiría JSON.stringify (los enteros sin decimales; NaN e infinito → null).
pub fn n(x: f64) -> String {
    if !x.is_finite() { return "null".into(); }
    if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") }
}

pub fn opt(x: Option<f64>) -> String { x.map(n).unwrap_or_else(|| "null".into()) }

/// Decodifica %XX y «+» de una cadena de consulta.
pub fn url_dec(x: &str) -> String {
    let b = x.as_bytes();
    let mut o = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => o.push(b' '),
            b'%' if i + 2 < b.len() => {
                let h = |c: u8| (c as char).to_digit(16);
                match (h(b[i + 1]), h(b[i + 2])) {
                    (Some(a), Some(c)) => { o.push((a * 16 + c) as u8); i += 2; }
                    _ => o.push(b'%'),
                }
            }
            c => o.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&o).into_owned()
}

pub fn consulta(q: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for par in q.split('&') {
        if par.is_empty() { continue; }
        let (k, v) = par.split_once('=').unwrap_or((par, ""));
        m.insert(url_dec(k), url_dec(v));
    }
    m
}
