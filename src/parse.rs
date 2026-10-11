//! Minimal, dependency-free Markdown + YAML-frontmatter parsing.
//!
//! This is intentionally NOT a full YAML parser. OKF v0.1 only requires us to
//! detect the frontmatter block, confirm it is terminated, and read top-level
//! scalar fields such as `type` and `timestamp`. Nested structures in the
//! frontmatter are ignored.

/// A top-level `key: value` pair found inside a frontmatter block.
#[derive(Debug, Clone)]
pub struct Field {
    pub key: String,
    pub value: String,
    /// 1-based line number of this field within the file.
    pub line: usize,
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
        return Doc { has_frontmatter: false, fm_closed: false, fields: Vec::new() };
    }

    let mut fm_closed = false;
    let mut fields = Vec::new();

    for (idx, line) in lines.iter().enumerate().skip(1) {
        if line.trim_end() == "---" {
            fm_closed = true;
            break;
        }
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
            if (value.starts_with('"') && value.ends_with('"') && value.len() > 1)
                || (value.starts_with('\'') && value.ends_with('\'') && value.len() > 1)
            {
                let inner = &value[1..value.len() - 1];
                value = inner.to_string();
            }
            if !key.is_empty() {
                fields.push(Field { key, value, line: idx + 1 });
            }
        }
    }

    Doc { has_frontmatter: true, fm_closed, fields }
}
