use std::collections::BTreeMap;

const MAX_DEPTH: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(u64),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    pub fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }
}

pub fn parse(input: &str) -> Result<JsonValue, String> {
    let mut parser = Parser { input, pos: 0 };
    let value = parser.value(0)?;
    parser.whitespace();
    if parser.pos != input.len() {
        return Err(format!("unexpected trailing data at byte {}", parser.pos));
    }
    Ok(value)
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn value(&mut self, depth: usize) -> Result<JsonValue, String> {
        if depth > MAX_DEPTH {
            return Err("JSON nesting is too deep".into());
        }
        self.whitespace();
        match self.peek() {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'"') => Ok(JsonValue::String(self.string()?)),
            Some(b't') => {
                self.literal("true")?;
                Ok(JsonValue::Bool(true))
            }
            Some(b'f') => {
                self.literal("false")?;
                Ok(JsonValue::Bool(false))
            }
            Some(b'n') => {
                self.literal("null")?;
                Ok(JsonValue::Null)
            }
            Some(b'0'..=b'9') => self.number(),
            Some(other) => Err(format!("unexpected byte {other:#04x}")),
            None => Err("unexpected end of JSON".into()),
        }
    }

    fn object(&mut self, depth: usize) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        self.whitespace();
        let mut object = BTreeMap::new();
        if self.consume(b'}') {
            return Ok(JsonValue::Object(object));
        }
        loop {
            self.whitespace();
            if self.peek() != Some(b'"') {
                return Err("object key must be a string".into());
            }
            let key = self.string()?;
            self.whitespace();
            self.expect(b':')?;
            let value = self.value(depth)?;
            if object.insert(key, value).is_some() {
                return Err("duplicate object key".into());
            }
            self.whitespace();
            if self.consume(b'}') {
                return Ok(JsonValue::Object(object));
            }
            self.expect(b',')?;
        }
    }

    fn array(&mut self, depth: usize) -> Result<JsonValue, String> {
        self.expect(b'[')?;
        self.whitespace();
        let mut array = Vec::new();
        if self.consume(b']') {
            return Ok(JsonValue::Array(array));
        }
        loop {
            array.push(self.value(depth)?);
            self.whitespace();
            if self.consume(b']') {
                return Ok(JsonValue::Array(array));
            }
            self.expect(b',')?;
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut output = String::new();
        loop {
            let ch = self.input[self.pos..]
                .chars()
                .next()
                .ok_or_else(|| "unterminated JSON string".to_owned())?;
            self.pos += ch.len_utf8();
            match ch {
                '"' => return Ok(output),
                '\\' => {
                    let escaped = self
                        .next_byte()
                        .ok_or_else(|| "unterminated JSON escape".to_owned())?;
                    match escaped {
                        b'"' => output.push('"'),
                        b'\\' => output.push('\\'),
                        b'/' => output.push('/'),
                        b'b' => output.push('\u{0008}'),
                        b'f' => output.push('\u{000c}'),
                        b'n' => output.push('\n'),
                        b'r' => output.push('\r'),
                        b't' => output.push('\t'),
                        b'u' => {
                            let first = self.hex_quad()?;
                            let code = if (0xD800..=0xDBFF).contains(&first) {
                                if self.next_byte() != Some(b'\\') || self.next_byte() != Some(b'u')
                                {
                                    return Err("missing low Unicode surrogate".into());
                                }
                                let second = self.hex_quad()?;
                                if !(0xDC00..=0xDFFF).contains(&second) {
                                    return Err("invalid low Unicode surrogate".into());
                                }
                                0x10000
                                    + (((first - 0xD800) as u32) << 10)
                                    + (second - 0xDC00) as u32
                            } else if (0xDC00..=0xDFFF).contains(&first) {
                                return Err("unexpected low Unicode surrogate".into());
                            } else {
                                first as u32
                            };
                            output.push(
                                char::from_u32(code)
                                    .ok_or_else(|| "invalid Unicode escape".to_owned())?,
                            );
                        }
                        other => return Err(format!("invalid JSON escape: {other:#04x}")),
                    }
                }
                ch if ch < '\u{0020}' => return Err("control character in JSON string".into()),
                ch => output.push(ch),
            }
        }
    }

    fn hex_quad(&mut self) -> Result<u16, String> {
        let mut value = 0u16;
        for _ in 0..4 {
            let byte = self
                .next_byte()
                .ok_or_else(|| "incomplete Unicode escape".to_owned())?;
            let digit = match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                other => return Err(format!("invalid Unicode hex digit: {other:#04x}")),
            };
            value = (value << 4) | u16::from(digit);
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.pos += 1;
        }
        let text = &self.input[start..self.pos];
        if text.len() > 1 && text.starts_with('0') {
            return Err("JSON numbers may not have leading zeros".into());
        }
        text.parse::<u64>()
            .map(JsonValue::Number)
            .map_err(|_| "JSON integer is out of range".into())
    }

    fn literal(&mut self, literal: &str) -> Result<(), String> {
        if self.input[self.pos..].starts_with(literal) {
            self.pos += literal.len();
            Ok(())
        } else {
            Err(format!("expected {literal}"))
        }
    }

    fn whitespace(&mut self) {
        while self
            .peek()
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.pos += 1;
        }
    }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(format!(
                "expected byte {expected:#04x} at byte {}",
                self.pos
            ))
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn next_byte(&mut self) -> Option<u8> {
        let byte = self.peek();
        if byte.is_some() {
            self.pos += 1;
        }
        byte
    }
}
