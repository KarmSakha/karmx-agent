//! Protobuf wire format for the browser-preview page bridge.
//!
//! Field numbers are taken directly from the integration script that runs
//! inside the proxied page (`cascade-browser-integration.js`). That script is
//! the source of truth for this protocol — if it changes, these structs must
//! change with it.
//!
//! Wire framing: every RPC body is a wrapper message with the real payload in
//! field 1, i.e. the page builds `w.message(1, requestField)` before POSTing.
//!
//! Hand-rolled decoder rather than prost so we need no `protoc` at build time.
//! Only the fields we actually consume are decoded; unknown fields are skipped.

/// A cursor over a protobuf-encoded byte slice.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

/// A decoded field: either a varint or a length-delimited slice.
pub enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }

    fn varint(&mut self) -> anyhow::Result<u64> {
        let mut out: u64 = 0;
        let mut shift = 0;
        loop {
            if self.pos >= self.buf.len() {
                anyhow::bail!("truncated varint");
            }
            let b = self.buf[self.pos];
            self.pos += 1;
            out |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                return Ok(out);
            }
            shift += 7;
            if shift >= 64 {
                anyhow::bail!("varint too long");
            }
        }
    }

    /// Advance to the next field, returning `(field_number, value)`.
    /// `None` at end of input. Unknown wire types are skipped.
    pub fn next_field(&mut self) -> anyhow::Result<Option<(u32, Field<'a>)>> {
        if self.is_empty() {
            return Ok(None);
        }
        let key = self.varint()?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match wire {
            0 => Ok(Some((field, Field::Varint(self.varint()?)))),
            2 => {
                let len = self.varint()? as usize;
                if self.pos + len > self.buf.len() {
                    anyhow::bail!("truncated length-delimited field");
                }
                let s = &self.buf[self.pos..self.pos + len];
                self.pos += len;
                Ok(Some((field, Field::Bytes(s))))
            }
            5 => {
                self.pos += 4;
                Ok(self.next_field()?)
            }
            1 => {
                self.pos += 8;
                Ok(self.next_field()?)
            }
            other => anyhow::bail!("unsupported wire type {other}"),
        }
    }
}

fn as_str(f: &Field<'_>) -> Option<String> {
    match f {
        Field::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Field::Varint(_) => None,
    }
}

/// `codeium_common_pb.FileLineRange`
#[derive(Debug, Default, Clone)]
pub struct FileLineRange {
    pub absolute_uri: String,
    pub start_line: u32,
}

impl FileLineRange {
    fn decode(buf: &[u8]) -> anyhow::Result<Self> {
        let mut out = Self::default();
        let mut r = Reader::new(buf);
        while let Some((n, f)) = r.next_field()? {
            match n {
                1 => out.absolute_uri = as_str(&f).unwrap_or_default(),
                2 => {
                    if let Field::Varint(v) = f {
                        out.start_line = v as u32;
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }
}

/// `codeium_common_pb.DOMElementScopeItem`
///
/// Field numbers from `encodeDomElement` in the page bridge.
#[derive(Debug, Default, Clone)]
pub struct DomElement {
    pub tag_name: String,
    pub outer_html: String,
    pub id: String,
    pub react_component_name: String,
    pub file_line_range: Option<FileLineRange>,
}

impl DomElement {
    pub fn decode(buf: &[u8]) -> anyhow::Result<Self> {
        let mut out = Self::default();
        let mut r = Reader::new(buf);
        while let Some((n, f)) = r.next_field()? {
            match n {
                1 => out.tag_name = as_str(&f).unwrap_or_default(),
                2 => out.outer_html = as_str(&f).unwrap_or_default(),
                3 => out.id = as_str(&f).unwrap_or_default(),
                4 => out.react_component_name = as_str(&f).unwrap_or_default(),
                5 => {
                    if let Field::Bytes(b) = f {
                        out.file_line_range = Some(FileLineRange::decode(b)?);
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }
}

/// `codeium_common_pb.ConsoleLogLine`
#[derive(Debug, Default, Clone)]
pub struct ConsoleLogLine {
    pub timestamp_str: String,
    pub kind: String,
    pub output: String,
}

/// `codeium_common_pb.ConsoleLogScopeItem`
#[derive(Debug, Default, Clone)]
pub struct ConsoleLog {
    pub lines: Vec<ConsoleLogLine>,
    pub server_address: String,
}

impl ConsoleLog {
    pub fn decode(buf: &[u8]) -> anyhow::Result<Self> {
        let mut out = Self::default();
        let mut r = Reader::new(buf);
        while let Some((n, f)) = r.next_field()? {
            match n {
                1 => {
                    if let Field::Bytes(b) = f {
                        let mut line = ConsoleLogLine::default();
                        let mut lr = Reader::new(b);
                        while let Some((ln, lf)) = lr.next_field()? {
                            match ln {
                                1 => line.timestamp_str = as_str(&lf).unwrap_or_default(),
                                2 => line.kind = as_str(&lf).unwrap_or_default(),
                                3 => line.output = as_str(&lf).unwrap_or_default(),
                                _ => {}
                            }
                        }
                        out.lines.push(line);
                    }
                }
                2 => out.server_address = as_str(&f).unwrap_or_default(),
                _ => {}
            }
        }
        Ok(out)
    }
}

/// Unwrap the Connect request envelope: the real payload lives in field 1.
pub fn unwrap_request(body: &[u8]) -> anyhow::Result<&[u8]> {
    let mut r = Reader::new(body);
    while let Some((n, f)) = r.next_field()? {
        if n == 1 {
            if let Field::Bytes(b) = f {
                return Ok(b);
            }
        }
    }
    anyhow::bail!("request envelope has no field 1 payload")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal encoder mirroring the page bridge's ProtoWriter.
    fn varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let mut b = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                b |= 0x80;
            }
            out.push(b);
            if v == 0 {
                return;
            }
        }
    }
    fn string_field(n: u32, s: &str, out: &mut Vec<u8>) {
        varint(((n as u64) << 3) | 2, out);
        varint(s.len() as u64, out);
        out.extend_from_slice(s.as_bytes());
    }

    #[test]
    fn decodes_dom_element_round_trip() {
        // encodeFileLineRange { absoluteUri: "/src/App.tsx", startLine: 42 }
        let mut flr = Vec::new();
        string_field(1, "/src/App.tsx", &mut flr);
        varint(2 << 3, &mut flr);
        varint(42, &mut flr);

        let mut el = Vec::new();
        string_field(1, "div", &mut el);
        string_field(2, "<div id=\"root\">hi</div>", &mut el);
        string_field(3, "root", &mut el);
        string_field(4, "App", &mut el);
        varint((5 << 3) | 2, &mut el);
        varint(flr.len() as u64, &mut el);
        el.extend_from_slice(&flr);

        // wrap as the page does: w.message(1, requestField)
        let mut body = Vec::new();
        varint((1 << 3) | 2, &mut body);
        varint(el.len() as u64, &mut body);
        body.extend_from_slice(&el);

        let payload = unwrap_request(&body).expect("unwrap");
        let dom = DomElement::decode(payload).expect("decode");
        assert_eq!(dom.tag_name, "div");
        assert_eq!(dom.id, "root");
        assert_eq!(dom.react_component_name, "App");
        assert_eq!(dom.outer_html, "<div id=\"root\">hi</div>");
        let flr = dom.file_line_range.expect("file line range");
        assert_eq!(flr.absolute_uri, "/src/App.tsx");
        assert_eq!(flr.start_line, 42);
    }

    #[test]
    fn decodes_console_log_round_trip() {
        let mut line = Vec::new();
        string_field(1, "10:00:01", &mut line);
        string_field(2, "error", &mut line);
        string_field(3, "boom", &mut line);

        let mut log = Vec::new();
        varint((1 << 3) | 2, &mut log);
        varint(line.len() as u64, &mut log);
        log.extend_from_slice(&line);
        string_field(2, "localhost:3000", &mut log);

        let mut body = Vec::new();
        varint((1 << 3) | 2, &mut body);
        varint(log.len() as u64, &mut body);
        body.extend_from_slice(&log);

        let payload = unwrap_request(&body).expect("unwrap");
        let c = ConsoleLog::decode(payload).expect("decode");
        assert_eq!(c.server_address, "localhost:3000");
        assert_eq!(c.lines.len(), 1);
        assert_eq!(c.lines[0].kind, "error");
        assert_eq!(c.lines[0].output, "boom");
    }

    #[test]
    fn skips_unknown_fields() {
        let mut el = Vec::new();
        string_field(1, "span", &mut el);
        varint(9 << 3, &mut el); // unknown varint field
        varint(7, &mut el);
        let dom = DomElement::decode(&el).expect("decode");
        assert_eq!(dom.tag_name, "span");
    }
}
