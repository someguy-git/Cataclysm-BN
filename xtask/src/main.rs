//! `xtask` — repository-wide Rust orchestration, replacing the CMake helper
//! targets once the C++ tree is gone.
//!
//! Run with `cargo xtask <command>`.
//!
//! Currently supported:
//!   * `fmt`         — run `cargo fmt --all`
//!   * `check`       — run `cargo clippy --workspace --all-targets -D warnings`
//!   * `test`        — run `cargo test --workspace`
//!   * `conformance` — compare a golden (C++) dump with a Rust dump

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let task = args.next().unwrap_or_default();
    match task.as_str() {
        "fmt" => run("cargo", &["fmt", "--all"]),
        "check" => run(
            "cargo",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        "test" => run("cargo", &["test", "--workspace"]),
        "conformance" => conformance(args.collect()),
        "" => {
            eprintln!("usage: cargo xtask <fmt|check|test|conformance>");
            ExitCode::from(2)
        }
        other => {
            eprintln!("unknown xtask command: {other}");
            ExitCode::from(2)
        }
    }
}

/// Compare a C++ golden dump with a Rust dump, exiting non-zero on any
/// difference (the Phase 1 conformance check).
fn conformance(args: Vec<String>) -> ExitCode {
    let [golden, actual] = args.as_slice() else {
        eprintln!("usage: cargo xtask conformance <golden-dump> <rust-dump>");
        return ExitCode::from(2);
    };
    match cata_conformance::compare_files(golden, actual) {
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

fn run(program: &str, args: &[&str]) -> ExitCode {
    match Command::new(program).args(args).status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => ExitCode::from(u8::try_from(status.code().unwrap_or(1)).unwrap_or(1)),
        Err(err) => {
            eprintln!("failed to run {program}: {err}");
            ExitCode::FAILURE
        }
    }
}
