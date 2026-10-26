//! A small strict JSON reader (RFC 8259) so the runtime DLL carries no
//! third-party code. Enough for the style files: objects keep key order, numbers
//! are f64, and errors name the byte offset.
use std::fmt::Write;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        if let Value::Num(n) = self { Some(*n) } else { None }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Value::Str(s) = self { Some(s) } else { None }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        if let Value::Arr(a) = self { Some(a) } else { None }
    }
    pub fn as_object(&self) -> Option<&[(String, Value)]> {
        if let Value::Obj(o) = self { Some(o) } else { None }
    }
}

pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser { s: text.as_bytes(), i: 0, depth: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != p.s.len() {
        return Err(p.err("trailing characters"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    depth: u32,
}

impl Parser<'_> {
    fn err(&self, what: &str) -> String {
        format!("JSON: {what} at byte {}", self.i)
    }
    fn ws(&mut self) {
        while matches!(self.s.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }
    fn eat(&mut self, lit: &str) -> Result<(), String> {
        if self.s[self.i..].starts_with(lit.as_bytes()) {
            self.i += lit.len();
            Ok(())
        } else {
            Err(self.err(&format!("expected {lit}")))
        }
    }
    fn value(&mut self) -> Result<Value, String> {
        match self.s.get(self.i) {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Value::Str),
            Some(b't') => self.eat("true").map(|_| Value::Bool(true)),
            Some(b'f') => self.eat("false").map(|_| Value::Bool(false)),
            Some(b'n') => self.eat("null").map(|_| Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.err("expected a value")),
        }
    }
    fn nest(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > 64 { Err(self.err("nesting too deep")) } else { Ok(()) }
    }
    fn object(&mut self) -> Result<Value, String> {
        self.nest()?;
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.s.get(self.i) == Some(&b'}') {
            self.i += 1;
            self.depth -= 1;
            return Ok(Value::Obj(items));
        }
        loop {
            self.ws();
            if self.s.get(self.i) != Some(&b'"') {
                return Err(self.err("expected a key"));
            }
            let k = self.string()?;
            self.ws();
            self.eat(":")?;
            self.ws();
            items.push((k, self.value()?));
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    self.depth -= 1;
                    return Ok(Value::Obj(items));
                }
                _ => return Err(self.err("expected , or }")),
            }
        }
    }
    fn array(&mut self) -> Result<Value, String> {
        self.nest()?;
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.s.get(self.i) == Some(&b']') {
            self.i += 1;
            self.depth -= 1;
            return Ok(Value::Arr(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    self.depth -= 1;
                    return Ok(Value::Arr(items));
                }
                _ => return Err(self.err("expected , or ]")),
            }
        }
    }
    fn hex4(&mut self) -> Result<u32, String> {
        let h = self.s.get(self.i..self.i + 4).ok_or_else(|| self.err("short \\u escape"))?;
        let v =
            u32::from_str_radix(std::str::from_utf8(h).unwrap_or("x"), 16).map_err(|_| self.err("bad \\u escape"))?;
        self.i += 4;
        Ok(v)
    }
    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let start = self.i;
            while let Some(&c) = self.s.get(self.i) {
                if c == b'"' || c == b'\\' || c < 0x20 {
                    break;
                }
                self.i += 1;
            }
            out.push_str(std::str::from_utf8(&self.s[start..self.i]).map_err(|_| self.err("invalid UTF-8"))?);
            match self.s.get(self.i) {
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let e = *self.s.get(self.i).ok_or_else(|| self.err("short escape"))?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut cp = self.hex4()?;
                            if (0xD800..0xDC00).contains(&cp) {
                                self.eat("\\u")?;
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(self.err("bad surrogate pair"));
                                }
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                            }
                            out.push(char::from_u32(cp).ok_or_else(|| self.err("bad code point"))?);
                        }
                        _ => return Err(self.err("bad escape")),
                    }
                }
                _ => return Err(self.err("unterminated string")),
            }
        }
    }
    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        if self.s.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        while matches!(self.s.get(self.i), Some(b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')) {
            self.i += 1;
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("");
        let leading_zero = text.trim_start_matches('-').starts_with('0')
            && text.trim_start_matches('-').len() > 1
            && text.trim_start_matches('-').as_bytes()[1].is_ascii_digit();
        match text.parse::<f64>() {
            Ok(n) if n.is_finite() && !leading_zero && !text.ends_with('.') => Ok(Value::Num(n)),
            _ => Err(format!("JSON: bad number {text:?} at byte {start}")),
        }
    }
}

/// JSON string literal for `s` (used by tools that print JSON).
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Serialises `v` with two-space indentation (used by tools that print JSON).
pub fn to_string_pretty(v: &Value) -> String {
    fn go(v: &Value, depth: usize, out: &mut String) {
        let pad = |d: usize| "  ".repeat(d);
        match v {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Num(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => {
                let _ = write!(out, "{}", *n as i64);
            }
            Value::Num(n) => {
                let _ = write!(out, "{n}");
            }
            Value::Str(s) => out.push_str(&quote(s)),
            Value::Arr(a) if a.is_empty() => out.push_str("[]"),
            Value::Obj(o) if o.is_empty() => out.push_str("{}"),
            Value::Arr(a) => {
                out.push_str("[\n");
                for (i, x) in a.iter().enumerate() {
                    out.push_str(&pad(depth + 1));
                    go(x, depth + 1, out);
                    out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
                }
                out.push_str(&pad(depth));
                out.push(']');
            }
            Value::Obj(o) => {
                out.push_str("{\n");
                for (i, (k, x)) in o.iter().enumerate() {
                    out.push_str(&pad(depth + 1));
                    out.push_str(&quote(k));
                    out.push_str(": ");
                    go(x, depth + 1, out);
                    out.push_str(if i + 1 < o.len() { ",\n" } else { "\n" });
                }
                out.push_str(&pad(depth));
                out.push('}');
            }
        }
    }
    let mut out = String::new();
    go(v, 0, &mut out);
    out
}

/// Builds an object from (key, value) pairs.
pub fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Obj(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::Str(s.to_owned())
    }
}
impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::Str(s)
    }
}
impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Bool(b)
    }
}
impl From<usize> for Value {
    fn from(n: usize) -> Value {
        Value::Num(n as f64)
    }
}
impl From<u32> for Value {
    fn from(n: u32) -> Value {
        Value::Num(n as f64)
    }
}
impl From<u64> for Value {
    fn from(n: u64) -> Value {
        Value::Num(n as f64)
    }
}
impl From<Vec<Value>> for Value {
    fn from(a: Vec<Value>) -> Value {
        Value::Arr(a)
    }
}
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(o: Option<T>) -> Value {
        o.map_or(Value::Null, Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_documents() {
        let v = parse(r#" {"a": [1, -2.5e1, true, null], "b": {"c": "x\"yé😀"}} "#).unwrap();
        assert_eq!(v.get("a").unwrap().as_array().unwrap()[1].as_f64(), Some(-25.0));
        assert_eq!(v.get("b").unwrap().get("c").unwrap().as_str(), Some("x\"yé😀"));
    }

    #[test]
    fn rejects_malformed_input() {
        for bad in ["{", "[1,]", "{\"a\" 1}", "01", "1.", "\"\u{1}\"", "[1] x", "{\"a\":1,}", "nul"] {
            assert!(parse(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn pretty_output_parses_back() {
        let v = obj([
            ("a", 1u32.into()),
            ("b", vec![Value::from("x"), true.into(), Option::<u32>::None.into()].into()),
            ("c", Value::Num(0.5)),
        ]);
        assert_eq!(parse(&to_string_pretty(&v)).unwrap(), v);
    }

    #[test]
    fn quote_round_trips() {
        let s = "tab\t \"quote\" back\\slash \u{1}";
        assert_eq!(parse(&quote(s)).unwrap().as_str(), Some(s));
    }
}
