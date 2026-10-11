//! Minimal, dependency-free Markdown + YAML-frontmatter parsing.
//!
//! This is intentionally NOT a full YAML parser. OKF only requires us to
//! detect the frontmatter block, confirm it is terminated, and read top-level
//! scalar fields such as `type` and `timestamp`, plus the small structured
//! families introduced in v0.2 (`generated`, `verified`, `sources`,
//! `usage_window`, `parameters`, `executor`, `attester`). Nested structures
//! are handled for exactly those shapes (inline flow maps/lists and
//! indented block maps/lists); everything else is ignored.

/// A top-level `key: value` pair found inside a frontmatter block.
#[derive(Debug, Clone)]
pub struct Field {
    pub key: String,
    pub value: String,
    /// 1-based line number of this field within the file.
    pub line: usize,
}

/// One raw line inside the frontmatter block (between the `---` delimiters).
#[derive(Debug, Clone)]
pub struct FmLine {
    /// 1-based line number within the file.
    pub line: usize,
    /// Raw text of the line (without trailing newline).
    pub text: String,
}

impl FmLine {
    /// Width of the leading indentation in spaces (tabs count as 2).
    pub fn indent(&self) -> usize {
        self.text
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .map(|c| if c == '\t' { 2 } else { 1 })
            .sum()
    }

    pub fn trimmed(&self) -> &str {
        self.text.trim()
    }

    pub fn is_blank_or_comment(&self) -> bool {
        let t = self.trimmed();
        t.is_empty() || t.starts_with('#')
    }
}

/// A parsed document: its raw lines plus frontmatter metadata.
#[derive(Debug)]
pub struct Doc {
    /// True if the file begins with a `---` frontmatter delimiter.
    pub has_frontmatter: bool,
    /// True if the opened frontmatter block was closed with a `---`.
    pub fm_closed: bool,
    /// Top-level scalar fields parsed from the frontmatter.
    pub fields: Vec<Field>,
    /// Raw frontmatter lines (between delimiters), for structured v0.2 parsing.
    pub fm_lines: Vec<FmLine>,
}

impl Doc {
    pub fn field(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.key == key)
    }

}

/// Parse a document's frontmatter. The body itself is not interpreted here;
/// callers scan raw content for headings and links.
pub fn parse(content: &str) -> Doc {
    let lines: Vec<&str> = content.lines().collect();

    let starts_with_delim = lines
        .first()
        .map(|l| l.trim_end() == "---")
        .unwrap_or(false);

    if !starts_with_delim {
        return Doc { has_frontmatter: false, fm_closed: false, fields: Vec::new(), fm_lines: Vec::new() };
    }

    let mut fm_closed = false;
    let mut fields = Vec::new();
    let mut fm_lines = Vec::new();

    for (idx, line) in lines.iter().enumerate().skip(1) {
        if line.trim_end() == "---" {
            fm_closed = true;
            break;
        }
        fm_lines.push(FmLine { line: idx + 1, text: line.to_string() });
        // Only consider top-level scalar entries: no leading whitespace
        // (so nested keys / list items are skipped), and not a comment.
        if line.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.starts_with('-') {
            continue;
        }
        if let Some(colon) = line.find(':') {
            let key = line[..colon].trim().to_string();
            let mut value = line[colon + 1..].trim().to_string();
            // Strip surrounding quotes (single or double) from scalar values.
            value = strip_quotes(&value).to_string();
            if !key.is_empty() {
                fields.push(Field { key, value, line: idx + 1 });
            }
        }
    }

    Doc { has_frontmatter: true, fm_closed, fields, fm_lines }
}

/// Remove one layer of surrounding single/double quotes.
pub fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if s.len() > 1
        && ((s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\'')))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Parse an inline flow mapping `{ a: b, c: d }` into ordered pairs.
/// Returns an empty vec when `s` is not a `{...}` mapping.
pub fn parse_flow_map(s: &str) -> Vec<(String, String)> {
    let s = s.trim();
    if !(s.starts_with('{') && s.ends_with('}')) {
        return Vec::new();
    }
    let inner = &s[1..s.len() - 1];
    let mut out = Vec::new();
    for part in split_flow(inner, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(colon) = part.find(':') {
            let k = part[..colon].trim().to_string();
            let v = strip_quotes(part[colon + 1..].trim()).to_string();
            if !k.is_empty() {
                out.push((k, v));
            }
        }
    }
    out
}

/// Parse an inline flow list `[a, b, c]` into items.
/// Returns an empty vec when `s` is not a `[...]` list.
#[allow(dead_code)]
pub fn parse_flow_list(s: &str) -> Vec<String> {
    let s = s.trim();
    if !(s.starts_with('[') && s.ends_with(']')) {
        return Vec::new();
    }
    let inner = &s[1..s.len() - 1];
    split_flow(inner, ',')
        .iter()
        .map(|p| strip_quotes(p.trim()).to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Split on a delimiter, ignoring delimiters inside single/double quotes.
fn split_flow(s: &str, delim: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in s.chars() {
        if let Some(q) = quote {
            cur.push(c);
            if c == q {
                quote = None;
            }
        } else if c == '"' || c == '\'' {
            quote = Some(c);
            cur.push(c);
        } else if c == delim {
            parts.push(cur);
            cur = String::new();
        } else {
            cur.push(c);
        }
    }
    parts.push(cur);
    parts
}

/// Split a `key: value` line on its first colon. Returns `None` when there
/// is no colon or the key is empty. The value is quote-stripped.
pub fn split_kv(s: &str) -> Option<(String, String)> {
    let colon = s.find(':')?;
    let key = s[..colon].trim().to_string();
    if key.is_empty() {
        return None;
    }
    let value = strip_quotes(s[colon + 1..].trim()).to_string();
    Some((key, value))
}

/// The inline value and indented block belonging to top-level `key`.
///
/// Finds the top-level (unindented) `key:` line in the frontmatter and
/// returns its inline value plus every following indented line up to the
/// next top-level key. Returns `None` when the key is absent.
pub fn block_for(doc: &Doc, key: &str) -> Option<(String, usize, Vec<FmLine>)> {
    let mut start: Option<(String, usize, usize)> = None;
    for (i, l) in doc.fm_lines.iter().enumerate() {
        if l.indent() == 0 && !l.is_blank_or_comment() {
            if let Some((k, v)) = split_kv(l.trimmed()) {
                if k == key {
                    start = Some((v, l.line, i));
                    break;
                }
            }
        }
    }
    let (inline, key_line, idx) = start?;
    let mut block = Vec::new();
    for l in doc.fm_lines.iter().skip(idx + 1) {
        if l.is_blank_or_comment() {
            continue;
        }
        if l.indent() == 0 {
            break;
        }
        block.push(l.clone());
    }
    Some((inline, key_line, block))
}

/// Parse indented block-map lines (`  key: value`) into `(key, value, line)`.
pub fn parse_block_map(block: &[FmLine]) -> Vec<(String, String, usize)> {
    let mut out = Vec::new();
    for l in block {
        let t = l.trimmed();
        if t.starts_with('-') {
            continue;
        }
        if let Some((k, v)) = split_kv(t) {
            out.push((k, v, l.line));
        }
    }
    out
}
