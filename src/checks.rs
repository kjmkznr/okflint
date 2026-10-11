//! The individual OKF v0.2 conformance checks.
//!
//! Diagnostic codes:
//! - OKF001  concept document is missing the required `type` field (error)
//! - OKF002  `index.md` frontmatter violates §8 (error)
//! - OKF003  frontmatter was opened with `---` but never closed (error)
//! - OKF004  `log.md` date heading is not an ISO 8601 `YYYY-MM-DD` (error)
//! - OKF005  `log.md` entries are not ordered newest-first  (warning)
//! - OKF006  `timestamp` field is not a valid ISO 8601 value  (warning; legacy
//!           fallback for `generated.at`, §13.1)
//! - OKF007  broken internal link  (warning; OKF consumers must tolerate)
//! - OKF008  `okf_version` in the bundle-root `index.md` is not a valid
//!           `<major>.<minor>` version  (warning)
//! - OKF009  `status` is not one of `draft | stable | deprecated`  (warning)
//! - OKF010  a v0.2 timestamp (`generated.at`, `verified[].at`, `stale_after`,
//!           `sources[].last_modified`, `usage_window.from/to`) is not an ISO
//!           8601 datetime with an explicit UTC offset  (warning)
//! - OKF011  an actor (`generated.by`, `verified[].by`, `sources[].author`)
//!           does not follow the actor convention in §7  (warning)
//! - OKF012  a v0.2 frontmatter family is malformed (a required subfield such
//!           as `generated.by`, `verified[].by/at`, `sources[].resource`,
//!           `parameters[].name`, `executor.resource`, `attester.resource` or
//!           `usage_window.from/to` is missing)  (warning)
//! - OKF013  `type: Attested Computation` is missing required `runtime`  (error)
//! - OKF014  an Attested Computation has neither an inline `# Computation`
//!           fence nor a `computation` path  (warning)
//! - OKF015  a footnote `[^id]` has no matching `sources[].id`  (warning)

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::parse::{self, Doc};

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

/// OKF002 — `index.md` frontmatter rules (§8).
///
/// - The bundle-root `index.md` MAY carry frontmatter, but only to declare
///   the OKF version (`okf_version`). Any other field is an error.
/// - A subdirectory `index.md` MUST contain no frontmatter at all; any
///   frontmatter field is an error.
pub fn check_index_frontmatter(rel: &str, doc: &Doc, is_root: bool, out: &mut Vec<Diagnostic>) {
    if !doc.has_frontmatter || !doc.fm_closed {
        return;
    }
    if is_root {
        // `doc.fields` holds every top-level key (block families such as
        // `sources:` appear with an empty inline value), so one loop covers
        // both scalar and block frontmatter.
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
    } else if doc.has_frontmatter {
        // Any frontmatter at all in a subdirectory index violates §8.
        let line = doc.fields.first().map(|f| f.line).unwrap_or(1);
        let name = doc.fields.first().map(|f| f.key.as_str()).unwrap_or("frontmatter");
        out.push(Diagnostic::error(
            rel,
            Some(line),
            "OKF002",
            format!("subdirectory `index.md` must not contain frontmatter; unexpected field `{name}` (only the bundle-root `index.md` may declare `okf_version`)"),
        ));
    }
}

/// OKF008 — `okf_version` in the bundle-root `index.md` must be a valid
/// `<major>.<minor>` version.
///
/// Only evaluated for the bundle root; a subdirectory `index.md` carrying
/// `okf_version` is already an OKF002 error. Unknown-but-well-formed versions
/// are tolerated with best-effort consumption (per spec §12), so only the
/// shape is checked.
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

/// Whether a string is a valid `<major>.<minor>` version (e.g. `"0.2"`).
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

/// OKF006 — a present legacy `timestamp` field must be ISO 8601.
///
/// `timestamp` is superseded by `generated.at` in v0.2 but remains a valid
/// fallback when `generated` is absent (§13.1), so it is still accepted and
/// only its shape is checked.
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

// ---------------------------------------------------------------------------
// v0.2 frontmatter families (§5, §7, §10)
// ---------------------------------------------------------------------------

/// A single `verified` entry.
#[derive(Debug, Clone)]
struct VerifiedEntry {
    by: Option<(String, usize)>,
    at: Option<(String, usize)>,
    line: usize,
}

/// A single `sources` entry.
#[derive(Debug, Clone)]
struct SourceEntry {
    line: usize,
    id: Option<(String, usize)>,
    resource: Option<(String, usize)>,
    author: Option<(String, usize)>,
    usage_count: Option<(String, usize)>,
    last_modified: Option<(String, usize)>,
    window_from: Option<(String, usize)>,
    window_to: Option<(String, usize)>,
}

/// Structured view of the v0.2 families, extracted once per document.
#[derive(Debug, Default)]
struct V2 {
    generated_by: Option<(String, usize)>,
    generated_at: Option<(String, usize)>,
    generated_present: bool,
    generated_malformed: Option<usize>,
    verified: Vec<VerifiedEntry>,
    verified_present: bool,
    verified_malformed: Option<usize>,
    sources: Vec<SourceEntry>,
    sources_present: bool,
    sources_malformed: Option<usize>,
    top_window_from: Option<(String, usize)>,
    top_window_to: Option<(String, usize)>,
    top_window_present: bool,
    top_window_malformed: Option<usize>,
    status: Option<(String, usize)>,
    stale_after: Option<(String, usize)>,
    runtime: Option<(String, usize)>,
    computation: Option<(String, usize)>,
    executor_resource: Option<(String, usize)>,
    executor_present: bool,
    executor_malformed: Option<usize>,
    attester_resource: Option<(String, usize)>,
    attester_present: bool,
    attester_malformed: Option<usize>,
    parameters: Vec<(Option<(String, usize)>, usize)>,
    parameters_present: bool,
    parameters_malformed: Option<usize>,
    concept_type: Option<String>,
}

/// Extract the v0.2 families from raw frontmatter lines.
fn extract_v2(doc: &Doc) -> V2 {
    let mut v = V2::default();
    if !doc.has_frontmatter || !doc.fm_closed {
        return v;
    }
    if let Some(f) = doc.field("type") {
        v.concept_type = Some(f.value.clone());
    }

    // status / stale_after / runtime / computation are plain scalars.
    for key in ["status", "stale_after", "runtime", "computation"] {
        if let Some(f) = doc.field(key) {
            let pair = (f.value.clone(), f.line);
            match key {
                "status" => v.status = Some(pair),
                "stale_after" => v.stale_after = Some(pair),
                "runtime" => v.runtime = Some(pair),
                "computation" => v.computation = Some(pair),
                _ => {}
            }
        }
    }
    // A block-form scalar (`runtime:` followed by nothing on nested lines)
    // still appears in `fields` with an empty value; that emptiness is
    // meaningful for OKF012/OKF013, so keep it as-is.

    // generated: { by, at } — inline flow or indented block map.
    if let Some((inline, key_line, block)) = parse::block_for(doc, "generated") {
        v.generated_present = true;
        if !inline.is_empty() {
            if inline.starts_with('{') {
                let map: HashMap<String, String> = parse::parse_flow_map(&inline).into_iter().collect();
                v.generated_by = map.get("by").map(|s| (s.clone(), key_line));
                v.generated_at = map.get("at").map(|s| (s.clone(), key_line));
                if !map.contains_key("by") && !map.contains_key("at") {
                    v.generated_malformed = Some(key_line);
                }
            } else {
                v.generated_malformed = Some(key_line);
            }
        } else if block.is_empty() {
            v.generated_malformed = Some(key_line);
        } else {
            let map = parse::parse_block_map(&block);
            v.generated_by = map.iter().find(|(k, _, _)| k == "by").map(|(_, val, ln)| (val.clone(), *ln));
            v.generated_at = map.iter().find(|(k, _, _)| k == "at").map(|(_, val, ln)| (val.clone(), *ln));
            if v.generated_by.is_none() && v.generated_at.is_none() {
                v.generated_malformed = Some(key_line);
            }
        }
    }

    // verified: bare { by, at } mapping OR a list of such mappings.
    if let Some((inline, key_line, block)) = parse::block_for(doc, "verified") {
        v.verified_present = true;
        if !inline.is_empty() {
            if inline.starts_with('{') {
                let map: HashMap<String, String> = parse::parse_flow_map(&inline).into_iter().collect();
                v.verified.push(VerifiedEntry {
                    line: key_line,
                    by: map.get("by").map(|s| (s.clone(), key_line)),
                    at: map.get("at").map(|s| (s.clone(), key_line)),
                });
            } else {
                v.verified_malformed = Some(key_line);
            }
        } else if block.is_empty() {
            v.verified_malformed = Some(key_line);
        } else if block.iter().any(|l| l.trimmed().starts_with('-')) {
            v.verified = parse_verified_list(&block);
            if v.verified.is_empty() {
                v.verified_malformed = Some(key_line);
            }
        } else {
            // Block-form bare mapping:
            //   verified:
            //     by: human:alice
            //     at: 2026-…Z
            let map = parse::parse_block_map(&block);
            if map.is_empty() {
                v.verified_malformed = Some(key_line);
            } else {
                let line = map.first().map(|(_, _, ln)| *ln).unwrap_or(key_line);
                v.verified.push(VerifiedEntry {
                    line,
                    by: map.iter().find(|(k, _, _)| k == "by").map(|(_, val, ln)| (val.clone(), *ln)),
                    at: map.iter().find(|(k, _, _)| k == "at").map(|(_, val, ln)| (val.clone(), *ln)),
                });
            }
        }
    }

    // sources: a list of mappings.
    if let Some((inline, key_line, block)) = parse::block_for(doc, "sources") {
        v.sources_present = true;
        if !inline.is_empty() {
            // An inline value for `sources:` is not a valid list.
            v.sources_malformed = Some(key_line);
        } else if block.is_empty() {
            v.sources_malformed = Some(key_line);
        } else {
            let entries = parse_source_list(&block);
            if entries.is_empty() {
                v.sources_malformed = Some(key_line);
            } else {
                v.sources = entries;
            }
        }
    }

    // usage_window as a top-level sibling of `sources`.
    if let Some((inline, key_line, block)) = parse::block_for(doc, "usage_window") {
        v.top_window_present = true;
        if !inline.is_empty() {
            if inline.starts_with('{') {
                let map: HashMap<String, String> = parse::parse_flow_map(&inline).into_iter().collect();
                v.top_window_from = map.get("from").map(|s| (s.clone(), key_line));
                v.top_window_to = map.get("to").map(|s| (s.clone(), key_line));
                if v.top_window_from.is_none() && v.top_window_to.is_none() {
                    v.top_window_malformed = Some(key_line);
                }
            } else {
                v.top_window_malformed = Some(key_line);
            }
        } else if block.is_empty() {
            v.top_window_malformed = Some(key_line);
        } else {
            let map = parse::parse_block_map(&block);
            v.top_window_from = map.iter().find(|(k, _, _)| k == "from").map(|(_, val, ln)| (val.clone(), *ln));
            v.top_window_to = map.iter().find(|(k, _, _)| k == "to").map(|(_, val, ln)| (val.clone(), *ln));
            if v.top_window_from.is_none() && v.top_window_to.is_none() {
                v.top_window_malformed = Some(key_line);
            }
        }
    }

    // parameters: a list of { name, type, required }.
    if let Some((inline, key_line, block)) = parse::block_for(doc, "parameters") {
        v.parameters_present = true;
        if !inline.is_empty() {
            v.parameters_malformed = Some(key_line);
        } else if block.is_empty() {
            v.parameters_malformed = Some(key_line);
        } else {
            let items = parse_parameters_list(&block);
            if items.is_empty() {
                v.parameters_malformed = Some(key_line);
            } else {
                v.parameters = items;
            }
        }
    }

    // executor: { resource, receipt } — inline flow or block map.
    if let Some((inline, key_line, block)) = parse::block_for(doc, "executor") {
        v.executor_present = true;
        if !inline.is_empty() {
            if inline.starts_with('{') {
                let map: HashMap<String, String> = parse::parse_flow_map(&inline).into_iter().collect();
                // `receipt: [...]` contains commas, which the naive flow-map
                // split would break; recover it with a targeted parse.
                let receipt_extra = inline.contains("receipt");
                v.executor_resource = map.get("resource").map(|s| (s.clone(), key_line));
                if v.executor_resource.is_none() && !receipt_extra {
                    v.executor_malformed = Some(key_line);
                }
                if v.executor_resource.is_none() && receipt_extra {
                    // Try to salvage `resource:` when `receipt:` holds a list.
                    if let Some(r) = salvage_resource_from_flow(&inline) {
                        v.executor_resource = Some((r, key_line));
                    } else {
                        v.executor_malformed = Some(key_line);
                    }
                }
            } else {
                v.executor_malformed = Some(key_line);
            }
        } else if block.is_empty() {
            v.executor_malformed = Some(key_line);
        } else {
            let map = parse::parse_block_map(&block);
            v.executor_resource = map.iter().find(|(k, _, _)| k == "resource").map(|(_, val, ln)| (val.clone(), *ln));
            if v.executor_resource.is_none() && map.is_empty() {
                v.executor_malformed = Some(key_line);
            }
        }
    }

    // attester: { resource } — inline flow or block map.
    if let Some((inline, key_line, block)) = parse::block_for(doc, "attester") {
        v.attester_present = true;
        if !inline.is_empty() {
            if inline.starts_with('{') {
                let map: HashMap<String, String> = parse::parse_flow_map(&inline).into_iter().collect();
                v.attester_resource = map.get("resource").map(|s| (s.clone(), key_line));
                if v.attester_resource.is_none() {
                    v.attester_malformed = Some(key_line);
                }
            } else {
                v.attester_malformed = Some(key_line);
            }
        } else if block.is_empty() {
            v.attester_malformed = Some(key_line);
        } else {
            let map = parse::parse_block_map(&block);
            v.attester_resource = map.iter().find(|(k, _, _)| k == "resource").map(|(_, val, ln)| (val.clone(), *ln));
            if v.attester_resource.is_none() && map.is_empty() {
                v.attester_malformed = Some(key_line);
            }
        }
    }

    v
}

/// Recover `resource:` from an inline flow map whose `receipt:` value is a
/// comma-containing `[...]` list (e.g. `executor: { resource: x, receipt:
/// [a, b] }`), which the simple comma split would otherwise mangle.
fn salvage_resource_from_flow(inline: &str) -> Option<String> {
    let s = inline.trim();
    let inner = s.strip_prefix('{')?.strip_suffix('}')?;
    // Find `resource:` then read up to the next top-level `, receipt` or `,receipt` or end.
    let pos = inner.find("resource")?;
    let after = &inner[pos..];
    let colon = after.find(':')?;
    let mut rest = after[colon + 1..].trim();
    // Cut at `, receipt:` boundary if present.
    for marker in [", receipt:", ",receipt:", ", receipt :", ", receipt "] {
        if let Some(p) = rest.find(marker) {
            rest = rest[..p].trim();
            break;
        }
    }
    let val = parse::strip_quotes(rest.trim().trim_end_matches(',').trim()).to_string();
    if val.is_empty() { None } else { Some(val) }
}

/// Group block lines under `verified:` into dash-separated items and parse
/// each as a `{ by, at }` mapping (flow or block form).
fn parse_verified_list(block: &[parse::FmLine]) -> Vec<VerifiedEntry> {
    let mut out = Vec::new();
    let mut cur: Vec<parse::FmLine> = Vec::new();
    let flush = |cur: &mut Vec<parse::FmLine>, out: &mut Vec<VerifiedEntry>| {
        if cur.is_empty() {
            return;
        }
        let first = cur[0].trimmed().to_string();
        let first_line = cur[0].line;
        let body = first.strip_prefix('-').unwrap_or(&first).trim().to_string();
        if body.starts_with('{') {
            let map: HashMap<String, String> = parse::parse_flow_map(&body).into_iter().collect();
            out.push(VerifiedEntry {
                line: first_line,
                by: map.get("by").map(|s| (s.clone(), first_line)),
                at: map.get("at").map(|s| (s.clone(), first_line)),
            });
        } else {
            // `- by: x` plus continuation lines, or `- { ... }` already handled.
            let mut pairs: Vec<(String, String, usize)> = Vec::new();
            if !body.is_empty() {
                if let Some((k, val)) = parse::split_kv(&body) {
                    pairs.push((k, val, first_line));
                }
            }
            for l in cur.iter().skip(1) {
                if let Some((k, val)) = parse::split_kv(l.trimmed()) {
                    pairs.push((k, val, l.line));
                }
            }
            out.push(VerifiedEntry {
                line: first_line,
                by: pairs.iter().find(|(k, _, _)| k == "by").map(|(_, val, ln)| (val.clone(), *ln)),
                at: pairs.iter().find(|(k, _, _)| k == "at").map(|(_, val, ln)| (val.clone(), *ln)),
            });
        }
        cur.clear();
    };
    for l in block {
        if l.trimmed().starts_with('-') {
            flush(&mut cur, &mut out);
            cur.push(l.clone());
        } else if !cur.is_empty() {
            cur.push(l.clone());
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// Group block lines under `sources:` into entries. Each entry is either a
/// flow map (`- { id: x, resource: y }`) or a block map (`- id: x` followed
/// by deeper-indented `key: value` lines, including an inline
/// `usage_window: { from, to }` override).
fn parse_source_list(block: &[parse::FmLine]) -> Vec<SourceEntry> {
    let mut out = Vec::new();
    let mut cur: Vec<parse::FmLine> = Vec::new();
    let flush = |cur: &mut Vec<parse::FmLine>, out: &mut Vec<SourceEntry>| {
        if cur.is_empty() {
            return;
        }
        let first_line = cur[0].line;
        let first = cur[0].trimmed().to_string();
        let body = first.strip_prefix('-').unwrap_or(&first).trim().to_string();
        let mut pairs: Vec<(String, String, usize)> = Vec::new();
        if body.starts_with('{') {
            for (k, val) in parse::parse_flow_map(&body) {
                pairs.push((k, val, first_line));
            }
        } else {
            if !body.is_empty() {
                if let Some((k, val)) = parse::split_kv(&body) {
                    pairs.push((k, val, first_line));
                }
            }
            for l in cur.iter().skip(1) {
                let t = l.trimmed();
                if t.starts_with('-') {
                    continue;
                }
                if let Some((k, val)) = parse::split_kv(t) {
                    pairs.push((k, val, l.line));
                }
            }
        }
        let get = |k: &str| pairs.iter().find(|(key, _, _)| key == k).map(|(_, val, ln)| (val.clone(), *ln));
        // A per-entry `usage_window: { from, to }` override (inline flow form).
        let (wfrom, wto) = match pairs.iter().find(|(k, _, _)| k == "usage_window") {
            Some((_, val, ln)) if val.starts_with('{') => {
                let m: HashMap<String, String> = parse::parse_flow_map(val).into_iter().collect();
                (m.get("from").map(|s| (s.clone(), *ln)), m.get("to").map(|s| (s.clone(), *ln)))
            }
            _ => (None, None),
        };
        // An empty `-` item carries no pairs; the caller treats an empty
        // overall list as malformed, so still record the shell entry.
        out.push(SourceEntry {
            line: first_line,
            id: get("id"),
            resource: get("resource"),
            author: get("author"),
            usage_count: get("usage_count"),
            last_modified: get("last_modified"),
            window_from: wfrom,
            window_to: wto,
        });
        cur.clear();
    };
    for l in block {
        if l.trimmed().starts_with('-') {
            flush(&mut cur, &mut out);
            cur.push(l.clone());
        } else if !cur.is_empty() {
            cur.push(l.clone());
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// Group block lines under `parameters:` into entries; only the `name` is
/// structurally required (OKF012), the rest is informational.
fn parse_parameters_list(block: &[parse::FmLine]) -> Vec<(Option<(String, usize)>, usize)> {
    let mut out = Vec::new();
    let mut cur: Vec<parse::FmLine> = Vec::new();
    let flush = |cur: &mut Vec<parse::FmLine>, out: &mut Vec<(Option<(String, usize)>, usize)>| {
        if cur.is_empty() {
            return;
        }
        let first_line = cur[0].line;
        let first = cur[0].trimmed().to_string();
        let body = first.strip_prefix('-').unwrap_or(&first).trim().to_string();
        let mut name: Option<(String, usize)> = None;
        if body.starts_with('{') {
            let map: HashMap<String, String> = parse::parse_flow_map(&body).into_iter().collect();
            name = map.get("name").map(|s| (s.clone(), first_line));
        } else {
            if !body.is_empty() {
                if let Some((k, val)) = parse::split_kv(&body) {
                    if k == "name" {
                        name = Some((val, first_line));
                    }
                }
            }
            for l in cur.iter().skip(1) {
                if let Some((k, val)) = parse::split_kv(l.trimmed()) {
                    if k == "name" && name.is_none() {
                        name = Some((val, l.line));
                    }
                }
            }
            // A bare `- year` scalar item has no `name:` key; treat the
            // scalar itself as malformed (missing name) rather than dropping it.
        }
        out.push((name, first_line));
        cur.clear();
    };
    for l in block {
        if l.trimmed().starts_with('-') {
            flush(&mut cur, &mut out);
            cur.push(l.clone());
        } else if !cur.is_empty() {
            cur.push(l.clone());
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// OKF009 — `status` must be `draft | stable | deprecated` (§5.4).
pub fn check_status(rel: &str, doc: &Doc, out: &mut Vec<Diagnostic>) {
    let v = extract_v2(doc);
    if let Some((val, line)) = v.status {
        if !val.is_empty() && !matches!(val.as_str(), "draft" | "stable" | "deprecated") {
            out.push(Diagnostic::warning(
                rel,
                Some(line),
                "OKF009",
                format!("`status` should be one of `draft | stable | deprecated`, found `{val}`"),
            ));
        }
    }
}

/// OKF010 — v0.2 timestamps must be ISO 8601 datetimes with an explicit UTC
/// offset (§5): `generated.at`, each `verified[].at`, `stale_after`, each
/// `sources[].last_modified`, and every `usage_window` `from`/`to`.
pub fn check_timestamps(rel: &str, doc: &Doc, out: &mut Vec<Diagnostic>) {
    let v = extract_v2(doc);
    let mut items: Vec<(&str, Option<(String, usize)>)> = Vec::new();
    items.push(("generated.at", v.generated_at));
    items.push(("stale_after", v.stale_after));
    items.push(("usage_window.from", v.top_window_from));
    items.push(("usage_window.to", v.top_window_to));
    for e in &v.verified {
        items.push(("verified.at", e.at.clone()));
    }
    for s in &v.sources {
        items.push(("sources.last_modified", s.last_modified.clone()));
        items.push(("sources.usage_window.from", s.window_from.clone()));
        items.push(("sources.usage_window.to", s.window_to.clone()));
    }
    for (name, opt) in items {
        if let Some((val, line)) = opt {
            if !val.is_empty() && !is_iso_datetime_with_offset(&val) {
                out.push(Diagnostic::warning(
                    rel,
                    Some(line),
                    "OKF010",
                    format!("`{name}` should be an ISO 8601 datetime with an explicit UTC offset (e.g. `2026-06-30T14:00:00Z`), found `{val}`"),
                ));
            }
        }
    }
}

/// OKF011 — actors must follow the §7 convention:
/// `<producer>/<version>` for agents/tools, `human:<id>` for people,
/// `process:<id>` for automated processes (plus the `team:<id>` form used
/// for source authorship in §5.1 examples).
pub fn check_actors(rel: &str, doc: &Doc, out: &mut Vec<Diagnostic>) {
    let v = extract_v2(doc);
    let mut items: Vec<(&str, Option<(String, usize)>)> = Vec::new();
    items.push(("generated.by", v.generated_by));
    for (i, e) in v.verified.iter().enumerate() {
        let _ = i;
        items.push(("verified.by", e.by.clone()));
    }
    for s in &v.sources {
        if let Some(author) = s.author.clone() {
            items.push(("sources.author", Some(author)));
        }
    }
    for (name, opt) in items {
        if let Some((val, line)) = opt {
            if !val.is_empty() && !is_valid_actor(&val) {
                out.push(Diagnostic::warning(
                    rel,
                    Some(line),
                    "OKF011",
                    format!("`{name}` should follow the actor convention `<producer>/<version>`, `human:<id>` or `process:<id>`, found `{val}`"),
                ));
            }
        }
    }
}

/// Whether a string follows the §7 actor convention (leniently accepting the
/// `team:<id>` form used for `sources[].author` in the spec examples).
pub fn is_valid_actor(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    for prefix in ["human:", "process:", "team:"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return !rest.is_empty() && !rest.contains(':') && !rest.contains('/');
        }
    }
    // `<producer>/<version>`: both sides non-empty, no spaces or colons.
    if let Some(slash) = s.find('/') {
        let (a, b) = s.split_at(slash);
        let b = &b[1..];
        return !a.is_empty() && !b.is_empty() && !a.contains(':') && !b.contains(':') && !b.contains('/');
    }
    false
}

/// OKF012 — a present v0.2 family with a missing required subfield (§5, §10):
/// `generated.by`, each `verified` entry's `by`/`at`, each `sources` entry's
/// `resource`, each `parameters` entry's `name`, `executor.resource`,
/// `attester.resource`, and `usage_window` `from`/`to`.
pub fn check_structure(rel: &str, doc: &Doc, out: &mut Vec<Diagnostic>) {
    let v = extract_v2(doc);

    if let Some(line) = v.generated_malformed {
        out.push(Diagnostic::warning(
            rel,
            Some(line),
            "OKF012",
            "`generated` should be a mapping with `by` (and `at`), e.g. `generated: { by: human:alice, at: 2026-06-20T22:53:05Z }`",
        ));
    } else if v.generated_present && v.generated_by.as_ref().map(|(s, _)| s.is_empty()).unwrap_or(true) {
        let line = v.generated_by.as_ref().map(|(_, l)| *l).unwrap_or_else(|| doc.field("generated").map(|f| f.line).unwrap_or(1));
        out.push(Diagnostic::warning(rel, Some(line), "OKF012", "`generated` is missing required subfield `by`"));
    }

    if let Some(line) = v.verified_malformed {
        out.push(Diagnostic::warning(
            rel,
            Some(line),
            "OKF012",
            "`verified` should be a `{ by, at }` mapping or a list of them, e.g. `verified: { by: human:alice, at: 2026-06-25T09:00:00Z }`",
        ));
    } else {
        for e in &v.verified {
            if e.by.as_ref().map(|(s, _)| s.is_empty()).unwrap_or(true) {
                out.push(Diagnostic::warning(rel, Some(e.line), "OKF012", "`verified` entry is missing required subfield `by`"));
            }
            if e.at.as_ref().map(|(s, _)| s.is_empty()).unwrap_or(true) {
                out.push(Diagnostic::warning(rel, Some(e.line), "OKF012", "`verified` entry is missing required subfield `at`"));
            }
        }
    }

    if let Some(line) = v.sources_malformed {
        out.push(Diagnostic::warning(
            rel,
            Some(line),
            "OKF012",
            "`sources` should be a list of entries with a `resource`, e.g. `- { id: handbook, resource: https://example.com }`",
        ));
    } else {
        for s in &v.sources {
            if s.resource.as_ref().map(|(val, _)| val.is_empty()).unwrap_or(true) {
                out.push(Diagnostic::warning(rel, Some(s.line), "OKF012", "`sources` entry is missing required subfield `resource`"));
            }
            if let Some((val, _)) = &s.usage_count {
                if !val.is_empty() && !is_non_negative_int(val) {
                    out.push(Diagnostic::warning(
                        rel,
                        Some(s.line),
                        "OKF012",
                        format!("`sources` entry `usage_count` should be a non-negative integer, found `{val}`"),
                    ));
                }
            }
            let has_from = s.window_from.as_ref().map(|(val, _)| !val.is_empty()).unwrap_or(false);
            let has_to = s.window_to.as_ref().map(|(val, _)| !val.is_empty()).unwrap_or(false);
            if has_from != has_to {
                out.push(Diagnostic::warning(
                    rel,
                    Some(s.line),
                    "OKF012",
                    "`sources` entry `usage_window` should carry both `from` and `to`",
                ));
            }
        }
    }

    if v.top_window_present {
        if let Some(line) = v.top_window_malformed {
            out.push(Diagnostic::warning(
                rel,
                Some(line),
                "OKF012",
                "`usage_window` should be a `{ from, to }` datetime range, e.g. `usage_window: { from: 2026-06-01T00:00:00Z, to: 2026-06-30T00:00:00Z }`",
            ));
        } else {
            let has_from = v.top_window_from.as_ref().map(|(val, _)| !val.is_empty()).unwrap_or(false);
            let has_to = v.top_window_to.as_ref().map(|(val, _)| !val.is_empty()).unwrap_or(false);
            if has_from != has_to {
                let line = v.top_window_from.as_ref().or(v.top_window_to.as_ref()).map(|(_, l)| *l).unwrap_or(1);
                out.push(Diagnostic::warning(
                    rel,
                    Some(line),
                    "OKF012",
                    "`usage_window` should carry both `from` and `to`",
                ));
            }
        }
    }

    if let Some(line) = v.parameters_malformed {
        out.push(Diagnostic::warning(
            rel,
            Some(line),
            "OKF012",
            "`parameters` should be a list of `{ name, type, required }` entries, e.g. `- { name: year, type: integer, required: true }`",
        ));
    } else {
        for (name, line) in &v.parameters {
            if name.as_ref().map(|(s, _)| s.is_empty()).unwrap_or(true) {
                out.push(Diagnostic::warning(rel, Some(*line), "OKF012", "`parameters` entry is missing required subfield `name`"));
            }
        }
    }

    if let Some(line) = v.executor_malformed {
        out.push(Diagnostic::warning(
            rel,
            Some(line),
            "OKF012",
            "`executor` should be a mapping with `resource`, e.g. `executor: { resource: references/skills/run.md }`",
        ));
    } else if v.executor_present && v.executor_resource.as_ref().map(|(s, _)| s.is_empty()).unwrap_or(true) {
        let line = v.executor_resource.as_ref().map(|(_, l)| *l).unwrap_or_else(|| doc.field("executor").map(|f| f.line).unwrap_or(1));
        out.push(Diagnostic::warning(rel, Some(line), "OKF012", "`executor` is missing required subfield `resource`"));
    }

    if let Some(line) = v.attester_malformed {
        out.push(Diagnostic::warning(
            rel,
            Some(line),
            "OKF012",
            "`attester` should be a mapping with `resource`, e.g. `attester: { resource: references/attesters/check.py }`",
        ));
    } else if v.attester_present && v.attester_resource.as_ref().map(|(s, _)| s.is_empty()).unwrap_or(true) {
        let line = v.attester_resource.as_ref().map(|(_, l)| *l).unwrap_or_else(|| doc.field("attester").map(|f| f.line).unwrap_or(1));
        out.push(Diagnostic::warning(rel, Some(line), "OKF012", "`attester` is missing required subfield `resource`"));
    }
}

/// OKF013 / OKF014 — Attested Computation contract (§10).
///
/// - OKF013 (error): `type: Attested Computation` carries no `runtime`.
/// - OKF014 (warning): neither an inline `# Computation` fenced block nor a
///   `computation:` path names the computation.
pub fn check_attested(rel: &str, doc: &Doc, content: &str, out: &mut Vec<Diagnostic>) {
    let v = extract_v2(doc);
    let is_attested = matches!(v.concept_type.as_deref(), Some(t) if t.trim() == "Attested Computation");
    if !is_attested {
        return;
    }
    let runtime_missing = v.runtime.as_ref().map(|(s, _)| s.trim().is_empty()).unwrap_or(true);
    if runtime_missing {
        let line = v.runtime.as_ref().map(|(_, l)| *l).or_else(|| doc.field("type").map(|f| f.line)).unwrap_or(1);
        out.push(Diagnostic::error(
            rel,
            Some(line),
            "OKF013",
            "`type: Attested Computation` requires a `runtime` field (e.g. `runtime: bigquery`)",
        ));
    }
    let has_path = v.computation.as_ref().map(|(s, _)| !s.trim().is_empty()).unwrap_or(false);
    if !has_path && !has_computation_fence(content) {
        out.push(Diagnostic::warning(
            rel,
            Some(1),
            "OKF014",
            "attested computation names no computation: provide an inline fenced block under `# Computation` or a `computation:` path",
        ));
    }
}

/// Whether the body carries a fenced code block under a `# Computation`
/// heading (§10.3). Only the presence of *some* fence after the heading is
/// required; the fence language is unrestricted.
fn has_computation_fence(content: &str) -> bool {
    let mut in_section = false;
    let mut in_fence = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            // A top-level `# Computation` heading opens the section; any
            // other heading of equal-or-higher level closes it.
            let hashes = t.chars().take_while(|c| *c == '#').count();
            let title = t[hashes..].trim().to_lowercase();
            if hashes == 1 && title == "computation" {
                in_section = true;
                continue;
            }
            if in_section && hashes == 1 {
                break;
            }
        }
        if in_section {
            if t.starts_with("```") || t.starts_with("~~~") {
                in_fence = !in_fence;
                if !in_fence {
                    return true;
                }
                // An opening fence followed later by its close counts; keep scanning.
                continue;
            }
            // Indented code blocks count as well.
            if line.starts_with("    ") && !line.trim().is_empty() {
                return true;
            }
        }
    }
    // An unclosed fence still names a computation; don't report OKF014 for it.
    in_fence
}

/// OKF015 — per-claim attribution (§5.1): every `[^id]` footnote in the body
/// should resolve to a `sources[].id`. Only checked when the document
/// declares `sources`; otherwise footnotes are ordinary markdown.
pub fn check_footnotes(rel: &str, doc: &Doc, content: &str, out: &mut Vec<Diagnostic>) {
    let v = extract_v2(doc);
    if !v.sources_present || v.sources.is_empty() {
        return;
    }
    let ids: HashSet<String> = v.sources.iter().filter_map(|s| s.id.clone()).map(|(id, _)| id).collect();
    if ids.is_empty() {
        return;
    }
    for (idx, line) in content.lines().enumerate() {
        // Skip footnote definitions (`[^id]: ...`); only uses are checked.
        // (A definition without a use is harmless and common.)
        let mut search = line;
        // Strip a leading definition marker so `[^id]: text [^other]` only
        // checks `[^other]`.
        if let Some(stripped) = search.trim_start().strip_prefix("[^") {
            if let Some(end) = stripped.find("]:") {
                search = &search[search.len() - search.trim_start().len() + "[^".len() + end + 2..];
            }
        }
        for label in extract_footnote_refs(search) {
            if !ids.contains(&label) {
                out.push(Diagnostic::warning(
                    rel,
                    Some(idx + 1),
                    "OKF015",
                    format!("footnote `[^{}]` has no matching `sources[].id`", label),
                ));
            }
        }
    }
}

fn extract_footnote_refs(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    let b = line.as_bytes();
    while i + 2 < b.len() {
        if b[i] == b'[' && b[i + 1] == b'^' {
            if let Some(end) = line[i + 2..].find(']') {
                let label = line[i + 2..i + 2 + end].trim().to_string();
                if !label.is_empty() && !label.contains(char::is_whitespace) {
                    out.push(label);
                }
                i = i + 2 + end + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn is_non_negative_int(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit())
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
/// The legacy `timestamp` fallback accepts a bare date; the v0.2 families in
/// OKF010 additionally require an explicit UTC offset (see below).
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

/// ISO 8601 datetime with an explicit UTC offset (§5), e.g.
/// `2026-06-30T14:00:00Z` or `2026-06-17T15:00:00+09:00`. A bare date or a
/// datetime without an offset is rejected.
pub fn is_iso_datetime_with_offset(s: &str) -> bool {
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
        return !rest.is_empty() && is_time(rest);
    }
    if let Some(pos) = rest.rfind(['+', '-']) {
        let (t, off) = rest.split_at(pos);
        if !is_offset(off) {
            return false;
        }
        return !t.is_empty() && is_time(t);
    }
    false
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
