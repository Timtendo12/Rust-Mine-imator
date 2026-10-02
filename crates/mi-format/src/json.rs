//! JSON reading and writing with the conventions of the original program.
//!
//! Reading is tolerant (BOM, raw control characters inside strings, invalid
//! UTF-8) because files in the wild were produced by several generations of
//! writers. Writing reproduces the layout of `json_save_*` exactly: tabs,
//! CRLF line breaks, numbers through `string_decimals`, `[ a, b, c ]` inline
//! arrays and `\uXXXX` escapes for everything outside ASCII.

use mi_core::Color;
use std::fmt;

/// A parsed JSON value. Objects keep the order of the file.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(JsonObject),
}

/// JSON object with keys in file order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JsonObject(Vec<(String, Json)>);

impl JsonObject {
    pub fn get(&self, key: &str) -> Option<&Json> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Json)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Nested object, if the key holds one (`ds_map_valid(map[?key])`).
    pub fn object(&self, key: &str) -> Option<&JsonObject> {
        self.get(key).and_then(Json::as_object)
    }

    /// Nested array, if the key holds one (`ds_list_valid(map[?key])`).
    pub fn array(&self, key: &str) -> Option<&[Json]> {
        self.get(key).and_then(Json::as_array)
    }

    /// `value_get_real`: numbers and booleans are accepted.
    pub fn real(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(Json::as_real)
    }

    /// `value_get_real` used for flags: true when the number is above 0.5,
    /// matching how GML evaluates numbers in conditions.
    pub fn flag(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(Json::as_flag)
    }

    /// `value_get_real` for values that may hold the string `"null"`.
    /// The outer `Option` is "key present and usable".
    pub fn nullable_real(&self, key: &str) -> Option<Option<f64>> {
        match self.get(key)? {
            Json::String(s) if s == "null" => Some(None),
            other => other.as_real().map(Some),
        }
    }

    /// `value_get_string`
    pub fn string(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Json::as_str)
    }

    /// `value_get_color`
    pub fn color(&self, key: &str) -> Option<Color> {
        self.string(key).map(Color::from_hex)
    }

    /// `value_get_point3D`: files store `[X, Z, Y]`; the result is `[x, y, z]`.
    pub fn point3(&self, key: &str) -> Option<[f64; 3]> {
        let list = self.array(key)?;
        if list.len() < 3 {
            return None;
        }
        Some([list[0].as_real()?, list[2].as_real()?, list[1].as_real()?])
    }

    /// `value_get_point2D`
    pub fn point2(&self, key: &str) -> Option<[f64; 2]> {
        let list = self.array(key)?;
        if list.len() < 2 {
            return None;
        }
        Some([list[0].as_real()?, list[1].as_real()?])
    }
}

impl FromIterator<(String, Json)> for JsonObject {
    fn from_iter<T: IntoIterator<Item = (String, Json)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl Json {
    pub fn as_object(&self) -> Option<&JsonObject> {
        match self {
            Json::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_real(&self) -> Option<f64> {
        match self {
            Json::Number(n) => Some(*n),
            Json::Bool(b) => Some(*b as u8 as f64),
            _ => None,
        }
    }

    pub fn as_flag(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            Json::Number(n) => Some(*n > 0.5),
            _ => None,
        }
    }
}

/// Error from [`parse`].
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message} on line {line}, column {column}")]
pub struct JsonError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

/// Maximum nesting depth accepted by [`parse`]; guards against stack
/// exhaustion on corrupted files.
const MAX_DEPTH: usize = 256;

/// Parses a JSON document whose root is an object or array.
pub fn parse(bytes: &[u8]) -> Result<Json, JsonError> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let mut parser = Parser { bytes, pos: 0 };
    parser.skip_whitespace();
    match parser.peek() {
        Some(b'{') | Some(b'[') => {}
        _ => return Err(parser.error("No root object found")),
    }
    let value = parser.value(0)?;
    parser.skip_whitespace();
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("Unexpected data after the root value"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> JsonError {
        let before = &self.bytes[..self.pos.min(self.bytes.len())];
        let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
        let column = before.iter().rev().take_while(|&&b| b != b'\n').count() + 1;
        JsonError { message: message.to_owned(), line, column }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8, message: &str) -> Result<(), JsonError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.error("Too deeply nested"));
        }
        match self.peek() {
            None => Err(self.error("Unexpected end of file")),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::String),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => self.word(),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.pos += 1;
        let mut entries = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                Some(b'"') => {}
                None => return Err(self.error("Unexpected end of file")),
                Some(_) => return Err(self.error("Expected a name")),
            }
            let name = self.string()?;
            self.skip_whitespace();
            self.expect(b':', "Expected \":\"")?;
            self.skip_whitespace();
            let value = self.value(depth + 1)?;
            entries.push((name, value));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                None => return Err(self.error("Unexpected end of file")),
                Some(_) => return Err(self.error("Expected \",\"")),
            }
        }
        Ok(Json::Object(JsonObject(entries)))
    }

    fn array(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                None => return Err(self.error("Unexpected end of file")),
                Some(_) => {}
            }
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                None => return Err(self.error("Unexpected end of file")),
                Some(_) => return Err(self.error("Expected \",\"")),
            }
        }
        Ok(Json::Array(items))
    }

    fn hex4(&mut self) -> Result<u16, JsonError> {
        let digits = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| self.error("Unexpected end of file"))?;
        let text = std::str::from_utf8(digits).map_err(|_| self.error("Invalid \\u escape"))?;
        let unit = u16::from_str_radix(text, 16).map_err(|_| self.error("Invalid \\u escape"))?;
        self.pos += 4;
        Ok(unit)
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.pos += 1;
        // Collected as UTF-16 where escapes are involved so that surrogate
        // pairs written as two \u escapes are joined.
        let mut out = String::new();
        let mut raw: Vec<u8> = Vec::new();
        let mut units: Vec<u16> = Vec::new();

        fn flush_raw(raw: &mut Vec<u8>, out: &mut String) {
            if !raw.is_empty() {
                out.push_str(&String::from_utf8_lossy(raw));
                raw.clear();
            }
        }
        fn flush_units(units: &mut Vec<u16>, out: &mut String) {
            if !units.is_empty() {
                out.push_str(&String::from_utf16_lossy(units));
                units.clear();
            }
        }

        loop {
            let Some(byte) = self.peek() else {
                return Err(self.error("Unexpected end of file"));
            };
            self.pos += 1;
            match byte {
                b'"' => break,
                b'\\' => {
                    let Some(escape) = self.peek() else {
                        return Err(self.error("Unexpected end of file"));
                    };
                    self.pos += 1;
                    if escape == b'u' {
                        flush_raw(&mut raw, &mut out);
                        units.push(self.hex4()?);
                        continue;
                    }
                    flush_units(&mut units, &mut out);
                    raw.push(match escape {
                        b'n' => b'\n',
                        b't' => b'\t',
                        b'r' => b'\r',
                        b'b' => 0x08,
                        b'f' => 0x0C,
                        other => other,
                    });
                }
                other => {
                    flush_units(&mut units, &mut out);
                    raw.push(other);
                }
            }
        }
        flush_raw(&mut raw, &mut out);
        flush_units(&mut units, &mut out);
        Ok(out)
    }

    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9' | b'.' | b'-' | b'+' | b'e' | b'E')) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("");
        match text.parse::<f64>() {
            Ok(n) if n.is_finite() => Ok(Json::Number(n)),
            _ => {
                self.pos = start;
                Err(self.error(&format!("Invalid number \"{text}\"")))
            }
        }
    }

    fn word(&mut self) -> Result<Json, JsonError> {
        for (word, value) in [("true", Json::Bool(true)), ("false", Json::Bool(false)), ("null", Json::Null)] {
            if self.bytes[self.pos..].starts_with(word.as_bytes()) {
                self.pos += word.len();
                return Ok(value);
            }
        }
        Err(self.error("Unrecognized word"))
    }
}

/// Formats a number like `string_decimals`: integers without a fraction,
/// everything else with up to five decimals and trailing zeroes removed.
pub fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    if value.floor() == value {
        return format!("{value:.0}");
    }
    let text = format!("{value:.5}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_owned()
}

/// A value that can be written on one line.
#[derive(Debug, Clone, PartialEq)]
pub enum Atom<'a> {
    Number(f64),
    Str(&'a str),
    /// Written as `[ a, b, c ]`.
    List(Vec<Atom<'a>>),
}

impl From<f64> for Atom<'_> {
    fn from(v: f64) -> Self {
        Atom::Number(v)
    }
}

impl From<i32> for Atom<'_> {
    fn from(v: i32) -> Self {
        Atom::Number(v as f64)
    }
}

impl<'a> From<&'a str> for Atom<'a> {
    fn from(v: &'a str) -> Self {
        Atom::Str(v)
    }
}

impl<'a> From<&'a String> for Atom<'a> {
    fn from(v: &'a String) -> Self {
        Atom::Str(v)
    }
}

/// Streaming writer equivalent to the `json_save_*` scripts.
#[derive(Debug, Default)]
pub struct JsonWriter {
    out: String,
    indent: usize,
    started: bool,
    add_comma: bool,
}

impl JsonWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Finishes writing and returns the document.
    pub fn finish(self) -> String {
        self.out
    }

    fn line_start(&mut self) {
        if self.add_comma {
            self.out.push(',');
        }
        self.out.push_str("\r\n");
        self.write_indent();
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push('\t');
        }
    }

    fn write_name(&mut self, name: &str) {
        self.out.push('"');
        self.out.push_str(name);
        self.out.push_str("\": ");
    }

    fn write_string(&mut self, s: &str) {
        self.out.push('"');
        encode_string(s, &mut self.out);
        self.out.push('"');
    }

    fn write_atom(&mut self, atom: &Atom<'_>) {
        match atom {
            Atom::Number(n) => self.out.push_str(&format_number(*n)),
            Atom::Str(s) => self.write_string(s),
            Atom::List(items) => {
                self.out.push_str("[ ");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.write_atom(item);
                }
                self.out.push_str(" ]");
            }
        }
    }

    /// `json_save_object_start`. The very first object of a document starts
    /// without a preceding line break.
    pub fn object_start(&mut self, name: Option<&str>) {
        if self.add_comma {
            self.out.push(',');
        }
        if self.started {
            self.out.push_str("\r\n");
        } else {
            self.started = true;
        }
        self.write_indent();
        if let Some(name) = name {
            self.write_name(name);
        }
        self.out.push('{');
        self.indent += 1;
        self.add_comma = false;
    }

    /// `json_save_object_done`
    pub fn object_done(&mut self) {
        self.close('}');
    }

    /// `json_save_array_start`
    pub fn array_start(&mut self, name: Option<&str>) {
        self.line_start();
        if let Some(name) = name {
            self.write_name(name);
        }
        self.out.push('[');
        self.indent += 1;
        self.add_comma = false;
    }

    /// `json_save_array_done`
    pub fn array_done(&mut self) {
        self.close(']');
    }

    fn close(&mut self, bracket: char) {
        self.out.push_str("\r\n");
        self.indent = self.indent.saturating_sub(1);
        self.write_indent();
        self.out.push(bracket);
        self.add_comma = true;
    }

    /// `json_save_array_value`
    pub fn array_value<'a>(&mut self, value: impl Into<Atom<'a>>) {
        self.line_start();
        self.write_atom(&value.into());
        self.add_comma = true;
    }

    /// `json_save_var`
    pub fn var<'a>(&mut self, name: &str, value: impl Into<Atom<'a>>) {
        self.line_start();
        self.write_name(name);
        self.write_atom(&value.into());
        self.add_comma = true;
    }

    /// `json_save_var_bool`: written as `true` / `false`.
    pub fn var_bool(&mut self, name: &str, value: bool) {
        self.line_start();
        self.write_name(name);
        self.out.push_str(if value { "true" } else { "false" });
        self.add_comma = true;
    }

    /// A flag that the original passes through `json_save_var`, which writes
    /// it as the number `1` or `0`.
    pub fn var_flag(&mut self, name: &str, value: bool) {
        self.var(name, value as u8 as f64);
    }

    /// `json_save_var_color`
    pub fn var_color(&mut self, name: &str, value: Color) {
        let hex = value.to_hex();
        self.var(name, hex.as_str());
    }

    /// `json_save_var_nullable` for numbers.
    pub fn var_nullable(&mut self, name: &str, value: Option<f64>) {
        match value {
            Some(v) => self.var(name, v),
            None => self.var(name, "null"),
        }
    }

    /// `json_save_var_point2D`
    pub fn var_point2(&mut self, name: &str, value: [f64; 2]) {
        self.var(name, Atom::List(vec![value[0].into(), value[1].into()]));
    }

    /// `json_save_var_point3D`: written as `[X, Z, Y]`.
    pub fn var_point3(&mut self, name: &str, value: [f64; 3]) {
        self.var(name, Atom::List(vec![value[0].into(), value[2].into(), value[1].into()]));
    }

    /// `json_save_var_point3D` with a null point.
    pub fn var_point3_nullable(&mut self, name: &str, value: Option<[f64; 3]>) {
        match value {
            Some(v) => self.var_point3(name, v),
            None => self.var(name, "null"),
        }
    }
}

/// Writes a parsed object back out under `name`, in file order.
pub(crate) fn write_object(w: &mut JsonWriter, name: &str, object: &JsonObject) {
    w.object_start(Some(name));
    for (key, value) in object.iter() {
        match value {
            Json::Null => w.var(key, "null"),
            Json::Bool(b) => w.var_bool(key, *b),
            Json::Number(n) => w.var(key, *n),
            Json::String(s) => w.var(key, s),
            Json::Object(o) => write_object(w, key, o),
            Json::Array(items) => {
                w.array_start(Some(key));
                for item in items {
                    match item {
                        Json::Number(n) => w.array_value(*n),
                        Json::String(s) => w.array_value(s),
                        Json::Bool(b) => w.array_value(*b as u8 as f64),
                        _ => {}
                    }
                }
                w.array_done();
            }
        }
    }
    w.object_done();
}

/// Escapes a string like `json_string_encode`: `\n`, `\t`, `"`, `\` and one
/// `\uXXXX` per UTF-16 unit for everything outside ASCII.
fn encode_string(s: &str, out: &mut String) {
    use fmt::Write;
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) > 127 => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_match_string_decimals() {
        assert_eq!(format_number(24.0), "24");
        assert_eq!(format_number(-100.0), "-100");
        assert_eq!(format_number(2.2), "2.2");
        assert_eq!(format_number(0.75), "0.75");
        assert_eq!(format_number(0.526), "0.526");
        assert_eq!(format_number(1.0 / 3.0), "0.33333");
        assert_eq!(format_number(0.123456789), "0.12346");
        assert_eq!(format_number(0.000001), "0");
        assert_eq!(format_number(-0.000001), "-0");
        assert_eq!(format_number(-0.0), "-0");
        assert_eq!(format_number(30000.0), "30000");
        assert_eq!(format_number(f64::NAN), "0");
    }

    #[test]
    fn writer_layout_matches_the_original() {
        let mut w = JsonWriter::new();
        w.object_start(None);
        w.var("format", 34);
        w.var("created_in", "2.0");
        w.object_start(Some("render"));
        w.var_bool("render_ssao", false);
        w.var_color("render_ssao_color", Color::BLACK);
        w.var_flag("render_shadows_transparent", false);
        w.var_point3("box", [200.0, 300.0, 400.0]);
        w.object_done();
        w.array_start(Some("types"));
        w.object_start(None);
        w.var("id", "A");
        w.object_done();
        w.object_start(None);
        w.var_nullable("region", None);
        w.object_done();
        w.array_done();
        w.array_start(Some("parts"));
        w.array_value("X");
        w.array_value(2.5);
        w.array_done();
        w.object_done();

        let expected = "{\r\n\t\"format\": 34,\r\n\t\"created_in\": \"2.0\",\r\n\t\"render\": {\r\n\
            \t\t\"render_ssao\": false,\r\n\t\t\"render_ssao_color\": \"#000000\",\r\n\
            \t\t\"render_shadows_transparent\": 0,\r\n\t\t\"box\": [ 200, 400, 300 ]\r\n\t},\r\n\
            \t\"types\": [\r\n\t\t{\r\n\t\t\t\"id\": \"A\"\r\n\t\t},\r\n\t\t{\r\n\
            \t\t\t\"region\": \"null\"\r\n\t\t}\r\n\t],\r\n\t\"parts\": [\r\n\t\t\"X\",\r\n\t\t2.5\r\n\t]\r\n}";
        assert_eq!(w.finish(), expected);
    }

    #[test]
    fn empty_containers() {
        let mut w = JsonWriter::new();
        w.object_start(None);
        w.array_start(Some("templates"));
        w.array_done();
        w.object_start(Some("keyframes"));
        w.object_done();
        w.var("list", Atom::List(vec![]));
        w.object_done();
        assert_eq!(
            w.finish(),
            "{\r\n\t\"templates\": [\r\n\t],\r\n\t\"keyframes\": {\r\n\t},\r\n\t\"list\": [  ]\r\n}"
        );
    }

    #[test]
    fn strings_are_escaped_like_json_string_encode() {
        let mut w = JsonWriter::new();
        w.object_start(None);
        w.var("name", "a\"b\\c\nd\té 😀");
        w.object_done();
        let text = w.finish();
        let escaped = [r"\u00e9", " ", r"\ud83d", r"\ude00"].concat();
        assert!(text.contains(&format!(r#""name": "a\"b\\c\nd\t{escaped}""#)), "{text}");

        let parsed = parse(text.as_bytes()).unwrap();
        assert_eq!(parsed.as_object().unwrap().string("name"), Some("a\"b\\c\nd\té 😀"));
    }

    #[test]
    fn parser_accepts_what_the_original_accepts() {
        let doc = b"\xEF\xBB\xBF{ \"a\": 1, \"b\": [true, false, null, -2.5e2], \"c\": {\"d\": \"x\ry\"}, \"e\": \"null\" }";
        let root = parse(doc).unwrap();
        let root = root.as_object().unwrap();
        assert_eq!(root.real("a"), Some(1.0));
        let b = root.array("b").unwrap();
        assert_eq!(b[0].as_real(), Some(1.0));
        assert_eq!(b[2], Json::Null);
        assert_eq!(b[3].as_real(), Some(-250.0));
        assert_eq!(root.object("c").unwrap().string("d"), Some("x\ry"));
        assert_eq!(root.nullable_real("e"), Some(None));
        assert_eq!(root.nullable_real("a"), Some(Some(1.0)));
        assert_eq!(root.nullable_real("missing"), None);
    }

    #[test]
    fn parser_reports_position() {
        let err = parse(b"{\n\t\"a\": 1\n\t\"b\": 2\n}").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.message.contains("Expected"));
        assert!(parse(b"").is_err());
        assert!(parse(b"12").is_err());
        assert!(parse(b"{\"a\": tru}").is_err());
        assert!(parse(b"{} x").is_err());
    }

    #[test]
    fn points_swap_y_and_z() {
        let root = parse(b"{\"p\": [1, 2, 3], \"q\": [4, 5]}").unwrap();
        let root = root.as_object().unwrap();
        assert_eq!(root.point3("p"), Some([1.0, 3.0, 2.0]));
        assert_eq!(root.point2("q"), Some([4.0, 5.0]));
        assert_eq!(root.point3("q"), None);
    }

    #[test]
    fn deep_nesting_is_rejected_not_overflowed() {
        let doc = "[".repeat(100_000);
        assert!(parse(doc.as_bytes()).is_err());
    }
}
