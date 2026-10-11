use std::collections::BTreeSet;
use std::path::Path;

use okflint::{counts, lint, Severity};

fn codes(dir: &str) -> BTreeSet<String> {
    lint(Path::new(dir))
        .unwrap()
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn valid_bundle_is_clean() {
    // A plain v0.1-style bundle stays clean under the v0.2 linter (§13:
    // a v0.1 bundle is consumable by a v0.2 consumer).
    let diags = lint(Path::new("tests/fixtures/valid")).unwrap();
    assert!(diags.is_empty(), "expected no findings, got: {diags:#?}");
}

#[test]
fn valid_v2_bundle_is_clean() {
    let diags = lint(Path::new("tests/fixtures/valid-v2")).unwrap();
    assert!(diags.is_empty(), "expected no findings, got: {diags:#?}");
}

#[test]
fn invalid_bundle_reports_each_rule() {
    let found = codes("tests/fixtures/invalid");
    for expected in ["OKF001", "OKF002", "OKF003", "OKF004", "OKF005", "OKF006", "OKF007", "OKF008"] {
        assert!(found.contains(expected), "expected {expected} in {found:?}");
    }
}

#[test]
fn invalid_v2_bundle_reports_each_new_rule() {
    let found = codes("tests/fixtures/invalid-v2");
    for expected in ["OKF002", "OKF009", "OKF010", "OKF011", "OKF012", "OKF013", "OKF014", "OKF015"] {
        assert!(found.contains(expected), "expected {expected} in {found:?}");
    }
}

#[test]
fn invalid_bundle_has_errors() {
    let diags = lint(Path::new("tests/fixtures/invalid")).unwrap();
    let (errors, warnings) = counts(&diags);
    assert!(errors > 0, "expected errors");
    assert!(warnings > 0, "expected warnings");
    assert!(diags.iter().any(|d| d.severity == Severity::Error));
}

#[test]
fn invalid_v2_bundle_has_errors() {
    // OKF013 (missing `runtime`) and OKF002 (subdir `index.md` frontmatter)
    // are errors; the remaining v0.2 findings are warnings.
    let diags = lint(Path::new("tests/fixtures/invalid-v2")).unwrap();
    let (errors, warnings) = counts(&diags);
    assert!(errors > 0, "expected errors, got: {diags:#?}");
    assert!(warnings > 0, "expected warnings, got: {diags:#?}");
    assert!(diags.iter().any(|d| d.code == "OKF013" && d.severity == Severity::Error));
}

#[test]
fn missing_root_is_an_error() {
    assert!(lint(Path::new("tests/fixtures/does-not-exist")).is_err());
}

#[test]
fn okflintignore_and_legacy_okfignore_are_both_honored() {
    let diags = lint(Path::new("tests/fixtures/ignored")).unwrap();
    assert!(diags.is_empty(), "expected no findings, got: {diags:#?}");
}
