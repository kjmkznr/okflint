//! The individual OKF v0.1 conformance checks.
//!
//! Diagnostic codes:
//! - OKF001  concept document is missing the required `type` field
//! - OKF002  `index.md` frontmatter contains non-`okf_version` fields
//! - OKF003  frontmatter was opened with `---` but never closed
//! - OKF004  `log.md` date heading is not an ISO 8601 `YYYY-MM-DD`
//! - OKF005  `log.md` entries are not ordered newest-first  (warning)
//! - OKF006  `timestamp` field is not a valid ISO 8601 value  (warning)
//! - OKF007  broken internal link  (warning; OKF consumers must tolerate)
//! - OKF008  `okf_version` in `index.md` is not a valid `<major>.<minor>` version  (warning)

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::parse::Doc;

/// OKF001 — every non-reserved document must declare a `type`.
pub fn check_concept_type(rel: &str, doc: &Doc, out: &mut Vec<Diagnostic>) {
    if !doc.has_frontmatter {
        out.push(Diagnostic::error(
            rel,
            Some(1),
            "OKF001",
            "concept document has no frontmatter; the `type` field is required",
        ));
        return;
    }
    // If frontmatter is unterminated, OKF003 already covers it; skip.
    if !doc.fm_closed {
        return;
    }
    match doc.field("type") {
        None => out.push(Diagnostic::error(rel, Some(1), "OKF001", "missing required frontmatter field `type`")),
        Some(f) if f.value.is_empty() => {
            out.push(Diagnostic::error(rel, Some(f.line), "OKF001", "frontmatter field `type` must not be empty"))
        }
        Some(_) => {}
    }
}

/// OKF002 — `index.md` frontmatter must only contain `okf_version`.
///
/// Per the spec, the root `index.md` MAY have frontmatter, but only to declare
/// the OKF version (`okf_version`). Any other field is an error.
pub fn check_index_frontmatter(rel: &str, doc: &Doc, out: &mut Vec<Diagnostic>) {
    if !doc.has_frontmatter || !doc.fm_closed {
        return;
    }
    for field in &doc.fields {
        if field.key != "okf_version" {
            out.push(Diagnostic::error(
                rel,
                Some(field.line),
                "OKF002",
                format!(
                    "`index.md` frontmatter may only contain `okf_version`; unexpected field `{}`",
                    field.key
                ),
            ));
        }
    }
}

/// OKF008 — `okf_version` in `index.md` must be a valid `<major>.<minor>` version.
///
/// If `okf_version` is present, its value should follow the `<major>.<minor>`
/// format (e.g. `"0.1"`). This is a warning because unknown versions are
/// tolerated via best-effort consumption (per spec).
pub fn check_okf_version(rel: &str, doc: &Doc, out: &mut Vec<Diagnostic>) {
    if let Some(f) = doc.field("okf_version") {
        if !is_valid_version(&f.value) {
            out.push(Diagnostic::warning(
                rel,
                Some(f.line),
                "OKF008",
                format!(
                    "`okf_version` should be a `<major>.<minor>` version string, found `{}`",
                    f.value
                ),
            ));
        }
    }
}

/// Whether a string is a valid `<major>.<minor>` version (e.g. `"0.1"`).
fn is_valid_version(s: &str) -> bool {
    let parts: Vec<&str> = s.splitn(2, '.').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return false;
    }
    parts[0].parse::<u32>().is_ok() && parts[1].parse::<u32>().is_ok()
}

/// OKF004 / OKF005 — `log.md` date headings must be ISO 8601 and newest-first.
pub fn check_log_headings(rel: &str, content: &str, out: &mut Vec<Diagnostic>) {
    let mut prev: Option<String> = None;
    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let Some(heading) = line.strip_prefix("## ") else { continue };
        let token = heading.trim().split_whitespace().next().unwrap_or("");
        if !is_iso_date(token) {
            out.push(Diagnostic::error(
                rel,
                Some(line_no),
                "OKF004",
                format!("`log.md` date heading must be ISO 8601 `YYYY-MM-DD`, found `{token}`"),
            ));
            continue;
        }
        if let Some(p) = &prev {
            if token > p.as_str() {
                out.push(Diagnostic::warning(
                    rel,
                    Some(line_no),
                    "OKF005",
                    format!("`log.md` entries should be newest-first; `{token}` is newer than the preceding `{p}`"),
                ));
            }
        }
        prev = Some(token.to_string());
    }
}

/// OKF006 — a present `timestamp` field must be ISO 8601.
pub fn check_timestamp(rel: &str, doc: &Doc, out: &mut Vec<Diagnostic>) {
    if let Some(f) = doc.field("timestamp") {
        if !f.value.is_empty() && !is_iso_8601(&f.value) {
            out.push(Diagnostic::warning(
                rel,
                Some(f.line),
                "OKF006",
                format!("`timestamp` is not a valid ISO 8601 value: `{}`", f.value),
            ));
        }
    }
}

/// OKF007 — internal `.md` links that do not resolve to a file.
///
/// Existence is checked against `files` (the precomputed set of all
/// bundle-relative paths) rather than a `stat` per link, so a link-dense bundle
/// costs no extra syscalls.
pub fn check_links(files: &HashSet<String>, rel: &str, content: &str, out: &mut Vec<Diagnostic>) {
    let file_dir = Path::new(rel).parent().map(Path::to_path_buf).unwrap_or_default();
    let mut in_code_block = false;
    for (idx, line) in content.lines().enumerate() {
        // Links inside fenced code blocks are examples, not real links.
        let fence = line.trim_start();
        if fence.starts_with("```") || fence.starts_with("~~~") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        for target in extract_link_targets(line) {
            let cleaned = target.split('#').next().unwrap_or("");
            if cleaned.is_empty() || target.contains("://") || target.starts_with("mailto:") {
                continue;
            }
            if !cleaned.contains(".md") {
                continue;
            }
            // Resolve to a bundle-relative, lexically-normalized key.
            let rel_path = if let Some(stripped) = cleaned.strip_prefix('/') {
                PathBuf::from(stripped)
            } else {
                file_dir.join(cleaned)
            };
            let key = normalize(&rel_path).to_string_lossy().replace('\\', "/");
            if !files.contains(&key) {
                out.push(Diagnostic::warning(
                    rel,
                    Some(idx + 1),
                    "OKF007",
                    format!("broken internal link: `{target}`"),
                ));
            }
        }
    }
}

/// Extract the URL portion of every `[text](url)` link on a line. Naive but
/// adequate: titles (`(url "title")`) and angle brackets are stripped.
fn extract_link_targets(line: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            if let Some(close) = line[i + 2..].find(')') {
                let inner = &line[i + 2..i + 2 + close];
                let target = inner
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_matches(|c| c == '<' || c == '>');
                if !target.is_empty() {
                    targets.push(target.to_string());
                }
                i = i + 2 + close + 1;
                continue;
            }
        }
        i += 1;
    }
    targets
}

/// Lexically normalize a path (resolve `.` and `..`) without touching the
/// filesystem, so existence checks work for `../` links.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        use std::path::Component::*;
        match comp {
            ParentDir => {
                out.pop();
            }
            CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `YYYY-MM-DD` with plausible month/day ranges.
pub fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits = |r: &[u8]| r.iter().all(u8::is_ascii_digit);
    if !digits(&b[0..4]) || !digits(&b[5..7]) || !digits(&b[8..10]) {
        return false;
    }
    let month: u32 = s[5..7].parse().unwrap_or(0);
    let day: u32 = s[8..10].parse().unwrap_or(0);
    (1..=12).contains(&month) && (1..=31).contains(&day)
}

/// ISO 8601 date or date-time, e.g. `2026-06-17` or `2026-06-17T15:00:00+09:00`.
pub fn is_iso_8601(s: &str) -> bool {
    if is_iso_date(s) {
        return true;
    }
    if s.len() < 11 || !is_iso_date(&s[0..10]) {
        return false;
    }
    let sep = s.as_bytes()[10];
    if sep != b'T' && sep != b' ' {
        return false;
    }
    let mut rest = &s[11..];
    if let Some(stripped) = rest.strip_suffix('Z') {
        rest = stripped;
    } else if let Some(pos) = rest.rfind(['+', '-']) {
        let (t, off) = rest.split_at(pos);
        if !is_offset(off) {
            return false;
        }
        rest = t;
    }
    is_time(rest)
}

fn is_time(s: &str) -> bool {
    let (hms, frac) = match s.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (s, None),
    };
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return false;
    }
    for (i, p) in parts.iter().enumerate() {
        if p.len() != 2 || !p.bytes().all(|c| c.is_ascii_digit()) {
            return false;
        }
        let v: u32 = p.parse().unwrap_or(99);
        if i == 0 && v > 23 {
            return false;
        }
        if i > 0 && v > 59 {
            return false;
        }
    }
    if let Some(f) = frac {
        if f.is_empty() || !f.bytes().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    true
}

fn is_offset(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || (b[0] != b'+' && b[0] != b'-') {
        return false;
    }
    let rest = &s[1..];
    let digits: String = rest.chars().filter(|&c| c != ':').collect();
    if !digits.bytes().all(|c| c.is_ascii_digit()) {
        return false;
    }
    matches!(rest.len(), 2 | 4 | 5)
}
