//! okflint — a dependency-free linter for Open Knowledge Format (OKF) v0.2 bundles.
//!
//! OKF v0.2 represents knowledge as a directory of Markdown files with YAML
//! frontmatter (see <https://github.com/GoogleCloudPlatform/knowledge-catalog>).
//! This crate validates a bundle against the spec's MUST rules (reported as
//! errors) and its SHOULD / tolerated rules (reported as warnings).

mod bundle;
mod checks;
mod diagnostic;
mod parse;

pub use diagnostic::{Diagnostic, Severity};

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

/// Below this file count, the thread-spawn overhead outweighs the gain, so we
/// lint sequentially.
const PARALLEL_THRESHOLD: usize = 64;

/// Lint the OKF bundle rooted at `root`. Returns all findings sorted by path
/// and line. Returns an `io::Error` only for problems reading the bundle root
/// itself (missing directory, etc.); per-file read errors become diagnostics.
///
/// Work is I/O-bound (one read per file). For large bundles the per-file checks
/// run across worker threads so file reads overlap.
pub fn lint(root: &Path) -> io::Result<Vec<Diagnostic>> {
    let meta = fs::metadata(root)?;
    if !meta.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a directory", root.display()),
        ));
    }

    let ignore = bundle::Ignore::load(root);
    let files = bundle::collect_markdown(root, &ignore)?;
    // All bundle paths, for O(1) link-existence checks (no stat per link).
    let all_files = bundle::collect_all_files(root)?;

    let mut diags = if files.len() < PARALLEL_THRESHOLD {
        let mut out = Vec::new();
        for path in &files {
            check_file(root, path, &all_files, &mut out);
        }
        out
    } else {
        lint_parallel(root, &files, &all_files)
    };

    diags.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    Ok(diags)
}

/// Distribute the files across worker threads. Each worker produces its own
/// `Vec<Diagnostic>`; results are concatenated (and sorted by the caller).
fn lint_parallel(root: &Path, files: &[std::path::PathBuf], all_files: &HashSet<String>) -> Vec<Diagnostic> {
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(files.len())
        .max(1);
    let chunk_size = files.len().div_ceil(workers);

    let mut out = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    let mut local = Vec::new();
                    for path in chunk {
                        check_file(root, path, all_files, &mut local);
                    }
                    local
                })
            })
            .collect();
        for handle in handles {
            if let Ok(local) = handle.join() {
                out.extend(local);
            }
        }
    });
    out
}

/// Read and check a single file, appending any findings to `out`.
fn check_file(root: &Path, path: &Path, all_files: &HashSet<String>, out: &mut Vec<Diagnostic>) {
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            out.push(Diagnostic::error(&rel, None, "OKF000", format!("could not read file: {e}")));
            return;
        }
    };

    let doc = parse::parse(&content);
    let basename = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

    if doc.has_frontmatter && !doc.fm_closed {
        out.push(Diagnostic::error(&rel, Some(1), "OKF003", "frontmatter opened with `---` but never closed"));
    }

    match basename.as_str() {
        "index.md" => {
            let is_root = rel == "index.md";
            checks::check_index_frontmatter(&rel, &doc, is_root, out);
            if is_root {
                checks::check_okf_version(&rel, &doc, out);
            }
        }
        "log.md" => checks::check_log_headings(&rel, &content, out),
        _ => {
            checks::check_concept_type(&rel, &doc, out);
            // v0.2 trust / lifecycle / provenance / computation families (§5, §10).
            // All optional; absence is never a finding.
            checks::check_status(&rel, &doc, out);
            checks::check_timestamps(&rel, &doc, out);
            checks::check_actors(&rel, &doc, out);
            checks::check_structure(&rel, &doc, out);
            checks::check_attested(&rel, &doc, &content, out);
            checks::check_footnotes(&rel, &doc, &content, out);
        }
    }

    checks::check_timestamp(&rel, &doc, out);
    checks::check_links(all_files, &rel, &content, out);
}

/// Convenience summary: number of errors and warnings in a result set.
pub fn counts(diags: &[Diagnostic]) -> (usize, usize) {
    let errors = diags.iter().filter(|d| d.severity == Severity::Error).count();
    (errors, diags.len() - errors)
}
