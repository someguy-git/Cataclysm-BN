//! `cata-conformance` — compare a golden (C++) registry dump with a Rust dump.
//!
//! Usage:
//!   cata-conformance compare <golden-dump> <rust-dump>
//!
//! Exits 0 when the dumps match, 1 when they differ, and 2 on usage or I/O
//! errors. The golden dump for a domain is produced by the C++ game once the
//! corresponding dump command exists; until then, domains are compared in
//! tests against fixtures.

use std::process::ExitCode;

use cata_conformance::compare_files;

const USAGE: &str = "usage: cata-conformance compare <golden-dump> <rust-dump>";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    match command.as_str() {
        "compare" => {
            let (Some(golden), Some(actual)) = (args.next(), args.next()) else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            compare(&golden, &actual)
        }
        "-h" | "--help" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        "" => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        other => {
            eprintln!("unknown command: {other}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn compare(golden: &str, actual: &str) -> ExitCode {
    match compare_files(golden, actual) {
        Ok(Ok(report)) => {
            print!("{}", report.summary());
            if report.is_match() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Ok(Err(error)) => {
            eprintln!("malformed dump: {error}");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("failed to read dump: {error}");
            ExitCode::from(2)
        }
    }
}
