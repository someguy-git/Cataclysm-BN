//! `xtask` — repository-wide Rust orchestration, replacing the CMake helper
//! targets once the C++ tree is gone.
//!
//! This is a placeholder for Phase 0. Run with `cargo xtask <command>`.
//!
//! Currently supported:
//!   * `fmt`   — run `cargo fmt --all`
//!   * `check` — run `cargo clippy --workspace --all-targets`

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
        "" => {
            eprintln!("usage: cargo xtask <fmt|check>");
            ExitCode::from(2)
        }
        other => {
            eprintln!("unknown xtask command: {other}");
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
