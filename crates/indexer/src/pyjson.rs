//! Byte-exact reproduction of Python's `json.dumps` for the indexer's
//! document data.
//!
//! The reference index stores `json.dumps(trait.data)` (see
//! `~/project/genenetwork3/scripts/index-genenetwork`). The twin gate
//! compares document data byte-for-byte, so serde_json's defaults are
//! not usable: Python emits `", "` / `": "` separators, keeps dict
//! insertion order, escapes every non-ASCII character as `\uXXXX`
//! (`ensure_ascii=True`), and formats floats with `repr` (shortest
//! round-trip, fixed notation for decimal exponents in `[-4, 16)` and
//! scientific `d.ddde±XX` outside it).
//!
//! [`JsonObject`] builds one document's data in insertion order from
//! typed values; [`escape_string`] and [`float_repr`] are the two
//! primitives that must match CPython exactly.

use std::fmt::Write as _;

/// One value in a document-data object.
pub enum JsonValue {
    Str(String),
    Int(i64),
    Float(f64),
    StrList(Vec<String>),
}

/// Builds a JSON object in insertion order with Python's separators.
#[derive(Default)]
pub struct JsonObject {
    entries: Vec<(String, JsonValue)>,
}

impl JsonObject {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append `key: value`; insertion order is the output order.
    pub fn push(&mut self, key: &str, value: JsonValue) {
        self.entries.push((key.to_string(), value));
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Render the object exactly as `json.dumps` would.
    pub fn to_string(&self) -> String {
        let mut out = String::new();
        out.push('{');
        for (index, (key, value)) in self.entries.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push('"');
            escape_string(key, &mut out);
            out.push_str("\": ");
            write_value(value, &mut out);
        }
        out.push('}');
        out
    }
}

fn write_value(value: &JsonValue, out: &mut String) {
    match value {
        JsonValue::Str(text) => {
            out.push('"');
            escape_string(text, out);
            out.push('"');
        }
        JsonValue::Int(number) => {
            let _ = write!(out, "{number}");
        }
        JsonValue::Float(number) => out.push_str(&float_repr(*number)),
        JsonValue::StrList(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                out.push('"');
                escape_string(item, out);
                out.push('"');
            }
            out.push(']');
        }
    }
}

/// Escape a string exactly as Python's `json.dumps` with
/// `ensure_ascii=True`: the two-character escapes for the standard
/// control characters, `\u00XX` for other control characters, and
/// `\uXXXX` (a surrogate pair above the BMP) for every non-ASCII
/// character.
pub fn escape_string(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c if (c as u32) < 0x7f => out.push(c),
            c => {
                // ensure_ascii: BMP code units as \uXXXX, astral as a
                // UTF-16 surrogate pair.
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{:04x}", *unit);
                }
            }
        }
    }
}

/// Format an `f64` exactly as CPython's `repr` (and therefore
/// `json.dumps`).
///
/// CPython uses the shortest decimal string that round-trips, rounded
/// to nearest with ties to even, then chooses fixed notation when the
/// decimal exponent `e` (value = `d.ddd × 10^e`) satisfies
/// `-4 <= e < 16`, and scientific `d.ddde±XX` otherwise.
///
/// Rust's `{:e}` is shortest but rounds ties away from zero (e.g.
/// `859.44000244140625` becomes `859.4400024414063`, CPython gives
/// `859.4400024414062`). Instead, search for the smallest significant
/// digit count whose round-half-even rendering round-trips; that is
/// exactly CPython's choice, and the reference docdata has such ties.
pub fn float_repr(value: f64) -> String {
    if value == 0.0 {
        // Handles +0.0 and -0.0 (repr(-0.0) == "-0.0").
        return if value.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        };
    }
    let mut rendered = None;
    for significant in 1..=17u32 {
        let candidate = format!("{:.*e}", (significant - 1) as usize, value);
        if candidate.parse::<f64>() == Ok(value) {
            rendered = Some(candidate);
            break;
        }
    }
    let rendered = rendered.expect("17 significant digits always round-trip an f64");
    let (mantissa, exponent) = rendered
        .split_once('e')
        .expect("LowerExp output always contains 'e'");
    let exponent: i32 = exponent.parse().expect("LowerExp exponent is an integer");

    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();

    let mut out = String::new();
    out.push_str(sign);
    if (-4..16).contains(&exponent) {
        if exponent >= 0 {
            let integer_digits = exponent as usize + 1;
            if digits.len() > integer_digits {
                out.push_str(&digits[..integer_digits]);
                out.push('.');
                out.push_str(&digits[integer_digits..]);
            } else {
                out.push_str(&digits);
                for _ in digits.len()..integer_digits {
                    out.push('0');
                }
                out.push_str(".0");
            }
        } else {
            out.push_str("0.");
            for _ in 0..(-exponent - 1) {
                out.push('0');
            }
            out.push_str(&digits);
        }
    } else {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push(if exponent < 0 { '-' } else { '+' });
        let magnitude = exponent.unsigned_abs();
        if magnitude < 10 {
            let _ = write!(out, "0{magnitude}");
        } else {
            let _ = write!(out, "{magnitude}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values sampled verbatim from the reference index docdata
    /// (`logs/probe-docdata.log`, `logs/scan-docdata.log`): the exact
    /// byte forms Python produced, including the float edge shapes
    /// (integral floats keep `.0`, tiny values go scientific).
    #[test]
    fn float_repr_matches_reference_bytes() {
        let cases: &[(f64, &str)] = &[
            (52.13529418496525, "52.13529418496525"),
            (44.970689, "44.970689"),
            (8.14033666666667, "8.14033666666667"),
            (22.004269639323, "22.004269639323"),
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (20.0, "20.0"),
            (460.0, "460.0"),
            (-17.0, "-17.0"),
            (2.39444435069444, "2.39444435069444"),
            (-3844.23775308921, "-3844.23775308921"),
            (6350.554222735148, "6350.554222735148"),
            // exponent shapes observed in reference docdata
            (6.031019218040234e-08, "6.031019218040234e-08"),
            (-2.977410049149484e-09, "-2.977410049149484e-09"),
            (-1.4110948100234523e-11, "-1.4110948100234523e-11"),
            (-2.8504644121442524e-05, "-2.8504644121442524e-05"),
            (3.2393828682277513e-10, "3.2393828682277513e-10"),
            // boundary shapes
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (1e-4, "0.0001"),
            (1e-5, "1e-05"),
            (1.5e300, "1.5e+300"),
            // observed tie cases where Rust's `{:e}` rounds the wrong
            // way; CPython rounds half to even
            (859.44000244140625, "859.4400024414062"),
            (819.85333251953125, "819.8533325195312"),
            (636.80230712890625, "636.8023071289062"),
            (13.4032135009765625, "13.403213500976562"),
            (3709.29327392578125, "3709.2932739257812"),
            (8.1207427978515625, "8.120742797851562"),
        ];
        for (value, expected) in cases {
            assert_eq!(float_repr(*value), *expected, "for {value:?}");
        }
    }

    /// Every distinct float token shape from the reference docdata
    /// (all exponent and `.0` forms, plus a deterministic plain
    /// sample): parsing each token back to f64 and re-rendering must
    /// reproduce the exact token bytes. Fixture generated read-only
    /// from the reference index (`scripts/gen-float-fixture.py`,
    /// `logs/gen-float-fixture.log`).
    #[test]
    fn float_repr_matches_reference_fixture() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/pyfloat.txt");
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("skipping: float fixture not present at {path}");
            return;
        };
        let mut checked = 0usize;
        for line in text.lines() {
            let token = line.trim();
            if token.is_empty() {
                continue;
            }
            let value: f64 = token.parse().expect("fixture token is a float");
            assert_eq!(float_repr(value), token, "for {value:?}");
            checked += 1;
        }
        assert!(checked > 1000, "fixture unexpectedly small: {checked}");
    }

    #[test]
    fn escape_string_matches_python() {
        let mut out = String::new();
        escape_string("a\"b\\c\n\t\r\u{08}\u{0c}\u{01}", &mut out);
        assert_eq!(out, "a\\\"b\\\\c\\n\\t\\r\\b\\f\\u0001");
        out.clear();
        escape_string("parré", &mut out);
        assert_eq!(out, "parr\\u00e9");
    }

    #[test]
    fn object_matches_python_layout() {
        let mut object = JsonObject::new();
        object.push("species", JsonValue::Str("mouse".into()));
        object.push("year", JsonValue::Int(2001));
        object.push("mean", JsonValue::Float(52.13529418496525));
        object.push(
            "authors",
            JsonValue::StrList(vec!["Airey DC".into(), "Lu L".into()]),
        );
        assert_eq!(
            object.to_string(),
            "{\"species\": \"mouse\", \"year\": 2001, \"mean\": 52.13529418496525, \
             \"authors\": [\"Airey DC\", \"Lu L\"]}"
        );
    }
}
