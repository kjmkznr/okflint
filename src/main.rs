use std::path::PathBuf;
use std::process::ExitCode;

use okflint::{counts, lint};

const HELP: &str = "\
okflint — a linter for Open Knowledge Format (OKF) v0.2 bundles

USAGE:
    okflint [OPTIONS] [PATH]

ARGS:
    PATH    Bundle root to lint (default: current directory)

OPTIONS:
    -q, --quiet           Print only the summary, not individual findings
        --deny-warnings   Exit non-zero if there are warnings as well as errors
    -h, --help            Print this help
    -V, --version         Print version

EXIT CODES:
    0   no errors (warnings allowed unless --deny-warnings)
    1   one or more errors (or warnings with --deny-warnings)
    2   usage error / could not read the bundle";

fn main() -> ExitCode {
    let mut path: Option<PathBuf> = None;
    let mut quiet = false;
    let mut deny_warnings = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("okflint {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "-q" | "--quiet" => quiet = true,
            "--deny-warnings" => deny_warnings = true,
            other if other.starts_with('-') => {
                eprintln!("okflint: unknown option '{other}' (try --help)");
                return ExitCode::from(2);
            }
            other => {
                if path.is_some() {
                    eprintln!("okflint: more than one path given");
                    return ExitCode::from(2);
                }
                path = Some(PathBuf::from(other));
            }
        }
    }

    let root = path.unwrap_or_else(|| PathBuf::from("."));
    let diags = match lint(&root) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("okflint: {e}");
            return ExitCode::from(2);
        }
    };

    if !quiet {
        for d in &diags {
            println!("{d}");
        }
    }

    let (errors, warnings) = counts(&diags);
    eprintln!("okflint: {errors} error(s), {warnings} warning(s)");

    if errors > 0 || (deny_warnings && warnings > 0) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
