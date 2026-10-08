# Rust Rewrite Plan

## Overview

This document is a **proposal** for incrementally porting the Cataclysm: Bright Nights engine from
C++ to Rust. It is a planning artifact only — no code has been migrated, and the plan is expected to
be revised as prototypes are built.

The document records the measured scale of the existing codebase, the compatibility constraints that
make a naive "big bang" rewrite infeasible, a phased migration strategy, a target Rust architecture,
and the risks that must be managed.

## Goals and non-goals

### Goals

- Improve memory-safety and eliminate a large class of undefined behavior in the engine.
- Simplify the build and dependency story (replace the CMake + vcpkg + MSYS2 + Android jungle with a
  Cargo workspace).
- Preserve gameplay, save compatibility, the JSON content format, and the Lua modding API.
- Keep the game playable at the end of **every** phase.

### Non-goals

- Changing the game's data format or breaking existing mods.
- A gameplay redesign: this is a port, not a sequel.
- A rewrite of the bundled third-party C sources (Lua 5.4, pinyin) — those stay as-is behind FFI.

## Current codebase scale

Measured from the repository (excluding bundled `src/lua`, `src/sol`, and `src/third-party`):

| Artifact | Size |
| --- | --- |
| C++ engine (`src/`) | ~529,650 lines across 936 files (495 `.h`, 441 `.cpp`) |
| `data/json` content | 1,934 files, 37 MB |
| Bundled mods (`data/mods`) | 80 directories, 18 MB (total `data/` = 73 MB) |
| Lua scripts (`data/**/*.lua`) | 72 files |
| Catch2 tests | 176 `*_test.cpp` files |
| Documentation | 447 markdown files |
| Existing Rust | none (no `Cargo.toml` or `.rs` files) |

Runtime dependencies declared in CMake: SQLite3, zlib, SDL3 (plus SDL3_ttf/SDL3_image/SDL3_mixer for
tiles and sound), ncurses (curses build), the vendored Lua 5.4 interpreter, sol2, and pinyin.

## The real constraint: content compatibility

The engine's value lives in `data/json`, `data/mods`, and Lua. Mod authors and the 80 bundled mods
depend on a stable contract:

- **JSON schema** for every factory loaded through `generic_factory` / `item_factory`, including
  `copy-from` inheritance semantics and error/source-location reporting.
- **Lua API surface** exposed by `src/catalua_bindings_*.cpp` and the hooks in
  `src/catalua_hooks.cpp`.
- **Save format** (SQLite-backed world saves).

A Rust engine that changes any of these breaks the ecosystem. The port must therefore be **format-
and API-compatible**: the same JSON parses, the same Lua API exists, and the same save schema loads
(or is losslessly migrated).

## Strategy: incremental "strangler fig", not big bang

A from-scratch reimplementation of a 530k-line, fifteen-year-old, content-driven engine is a
multi-year, high-risk effort. Instead:

1. Build a Rust workspace that links with the existing C++ binary via a stable C ABI.
2. Move one subsystem at a time across that boundary.
3. Keep a single playable binary throughout; cut the main loop over only once the simulation core is
   ported.

This lets each subsystem be validated against the still-authoritative C++ implementation, and it
avoids a long period with no shippable product.

## Target architecture

The Rust source lives in `src/`, and the existing C++ tree moves down one level into `src/cpp/`. A
single Cargo workspace rooted at the repository root owns the Rust crates, which sit directly under
`src/`:

```text
Cargo.toml            # workspace root (members = src/*)
src/
  cata-core/          # IDs (type_id/string_id), enums, damage, units, calendar, rng, coordinates
  cata-json/          # serde models for every data/json domain + compatibility layer
  cata-data/          # generic_factory equivalent: type registry + loaders
  cata-world/         # map, submap, overmap, fields, vision/light, pathfinding
  cata-entity/        # creature/character/monster/item/vehicle game objects
  cata-sim/           # turn loop, activities, effects, weather, scent, explosions
  cata-ui/            # backend-agnostic UI model
  cata-render-sdl/    # SDL3 tile backend
  cata-render-tui/    # terminal backend
  cata-lua/           # mlua bindings preserving the existing Lua API
  cata-ffi/           # C ABI shim for co-existence with remaining C++
  cata-bn/            # binary entry point
  cpp/                # existing C++ engine, moved here from src/
xtask/                # build/format/json-lint/test orchestration (replaces CMake targets)
```

Keeping `src/` as the Rust home (rather than a new top-level `crates/`) preserves the historical
location of engine code and keeps the initial move small. The C++ tree and the Rust crates must not
share basenames, but are otherwise independent.

### The one-time C++ move

Moving the current `src/*` into `src/cpp/` is mechanical but touches every build entry point that
hard-codes `src/`:

- `CMakeLists.txt`: `add_subdirectory(src)`, the `version.h` writer, the `prefix.h.in` / `prefix.h`
  paths, and the HLSL glob under `src/shaders/`.
- `src/CMakeLists.txt` (which becomes `src/cpp/CMakeLists.txt`): the source and header globs, the
  `catalua_bindings*` glob, the `main.cpp` / `messages.cpp` paths, and the
  `target_include_directories(... ${CMAKE_SOURCE_DIR}/src)` calls — the include directory must become
  `${CMAKE_SOURCE_DIR}/src/cpp`.
- `android/app/jni/CMakeLists.txt`: `CATA_SRC_DIR`.
- `tools/format/CMakeLists.txt` and `build-scripts/format-cpp.sh`.
- `lang/update_pot.sh` and `lang/update_stats.sh`, which scan `src/`.
- `pch/main-pch.hpp` (and the `update-pch.sh` helper) must be re-pointed at the new paths.

The C++ sources include headers by basename (for example `#include "game.h"`) and rely on the include
search path, so moving the entire tree together keeps those includes valid as long as the include
directory is updated. The generated `src/version.h` and `src/prefix.h` stay within the moved tree.

The move is done once, at the start of Phase 0, while the C++ tree is still authoritative — this
keeps the subsequent Rust phases free of directory churn.

### Dependency mapping

| C++ / current dependency | Rust replacement |
| --- | --- |
| `json.h` / hand-rolled JSON | `serde` + `serde_json` (with a source-location error layer) |
| `generic_factory` / `item_factory` | `cata-data` registry + `strum`-style dispatch |
| SQLite3 | `rusqlite` |
| zlib | `flate2` |
| SDL3 (+ttf/image/mixer) | `sdl3` bindings / `sdl3-image` / `sdl3-ttf` |
| ncurses | `crossterm` / `ratatui` |
| Lua 5.4 + sol2 | `mlua` (keep the vendored interpreter version in lockstep) |
| Catch2 tests | ported to Rust `#[test]` / `rstest`, or run against FFI during transition |

### Design notes

- **No ECS rewrite.** The existing game-object model (private constructors, `detached_ptr`,
  `safe_reference` / `cache_reference`, see [Game Objects](../explanation/game_objects.md)) maps
  cleanly onto Rust ownership (`Box`, `Rc`, generational arenas). Preserve the semantics; do not chase
  a Bevy-style ECS.
- **Save format.** Keep SQLite and the existing schema, or provide a one-way migrator. Do not invent
  a new save format early.
- **Rendering.** SDL3 for the tiles path; a terminal backend for the curses path, both behind the
  backend-agnostic `cata-ui` model.

## Phase plan

Each phase has explicit exit criteria. The game must remain playable after every phase.

### Phase 0 — Foundation

Move the current C++ tree from `src/` to `src/cpp/` and re-point the build (see [The one-time C++
move](#the-one-time-c-move)). Stand up the Cargo workspace rooted at the repository root with members
under `src/`, a `cata-ffi` C ABI, and CMake integration (via `corrosion` or `cc`) so one binary builds
both C++ and Rust. Port the leaf modules with zero dependents: `rng`, `calendar`, `units`, `damage`,
`coordinates`, `type_id` / `string_id`, `cata_variant`.

- **Exit:** the game builds and runs identically from the moved C++ tree; identical numeric behavior;
  cross-language unit tests pass.

### Phase 1 — Data layer

Port `generic_factory` and the JSON loaders domain by domain behind the FFI, and build a **JSON
conformance harness** that diffs Rust-loaded type registries against C++ for all 1,934 files and the
bundled mods.

- **Exit:** `--jsonverify` and `--check-mods` produce identical results under Rust.

### Phase 2 — World simulation

Port `map` / `submap` / `overmap`, fields, vision and lighting (`shadowcasting`), pathfinding, and the
reality bubble. This is the hardest correctness surface; invest in deterministic replay tests.

- **Exit:** byte-identical (or hashed) state after N turns across a corpus of seeded replays.

### Phase 3 — Entities and the turn loop

Port character/monster/item/vehicle, activities, effects, combat, and crafting; cut the main loop over
to Rust (`game::do_turn()` in the current code).

- **Exit:** the `tests/` behavioral suite runs against the Rust implementation.

### Phase 4 — Frontends and Lua

Port the UI model and the tiles/TUI backends. Re-implement the `mlua` bindings and hooks, preserving
the Lua API, and keep the Lua documentation generation working.

- **Exit:** bundled mods load and a Lua-heavy mod runs unchanged.

### Phase 5 — Decommission C++

Remove the FFI shims and delete `src/cpp/` (including the generated `version.h` / `prefix.h`); port the
tooling (`tools/json_tools`, the clang-tidy plugin → clippy), the i18n pipeline (`lang/`), and packaging
(Android, Windows, macOS, flatpak). At this point all engine code lives in the Rust crates under
`src/`, and the Cargo workspace is the sole build system.

- **Exit:** release artifacts are produced without any C++ in the build.

## Implementation status

Progress recorded on this branch:

- **Phase 0 (foundation) — in progress.**
  - The C++ tree now lives in `src/cpp/`; the repository's build files, scripts, CI
    workflows, tooling configs, docs, and agent skills have been re-pointed.
  - The Cargo workspace is rooted at the repository root (see `Cargo.toml`), with the
    first crates under `src/`: `cata-core` (hosting the `rng` port) and `cata-ffi` (the
    C ABI shim), plus `xtask`.
  - CMake builds the Rust static library with Cargo and links it into the game and test
    targets, guarded by the `CATA_RUST` option.
  - The remaining Phase 0 leaf modules (`calendar`, `units`, `damage`, `coordinates`,
    `type_id` / `string_id`, `cata_variant`) are not yet ported.
- **Phase 1 (data layer) — not started.**

### Verification notes

- The Rust workspace is verified: `cargo fmt --check`, `cargo clippy -- -D warnings`, and
  `cargo test` all pass, and `cargo build` produces `target/<profile>/libcata_ffi.a`.
- The re-pointed C++ build and the CMake↔Cargo integration could **not** be exercised in
  the sandbox used for this change (no CMake and no C++23 compiler were available), so
  they remain unverified end to end and must be checked on a machine with a full
  toolchain. Preserve the include-path move (`src/` → `src/cpp/`) when re-running them.
- `rng` parity: the `minstd_rand0` engine and the `djb2` hash are exact; the distribution
  helpers are deterministic and unbiased but not yet bit-identical to libstdc++, which the
  Phase 1 conformance oracle will require.

## Cross-cutting decisions

- **Test oracle.** The existing Catch2 suite is the acceptance suite. Port it, or run it against the
  FFI. Add deterministic replay/hash tests — the single most valuable correctness investment.
- **Lint and format.** `clippy` + `rustfmt`, replacing the CMake `format` / `style-json-parallel`
  targets and the custom clang-tidy plugin.
- **Build.** Keep CMake as the top-level driver initially (Rust built through `cc`/`corrosion`, with the
  workspace root `Cargo.toml` listing the `src/` crates); move to Cargo + `xtask` once C++ is gone.
- **Platform parity.** Windows, macOS, Android, and FreeBSD are in scope; verify SDL3, SQLite, and
  terminal bindings on each.

## Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| Save/data-schema drift breaks mods | JSON conformance harness; freeze formats; one-way migrators |
| Deterministic simulation differences (float/RNG/iteration order) | Seeded replay tests comparing state hashes against C++ |
| Lua API incompatibility | Preserve `mlua` surface; test with real bundled mods |
| Build and platform sprawl | CI matrix per platform from Phase 0 |
| Contributor fragmentation (community writes C++) | Keep the C++ tree authoritative until Rust reaches parity; document contribution boundaries |

## Effort estimate

Realistically **15–30 engineer-years** for full parity with a team that already knows the game. A lean
incremental port with FFI can reach a "Rust core, C++ shell" state in roughly **1–2 years**. Estimates
are informed ranges, not measured outcomes.

## First concrete steps

1. Move the C++ tree from `src/` to `src/cpp/` and re-point the build; confirm the game still builds
   and runs.
2. Create the Cargo workspace at the repository root (members under `src/`) and CMake integration;
   build the game linking one trivial Rust function from `src/`.
3. Port `rng` and `calendar` (small, pure, highly testable) and diff outputs against C++.
4. Build the JSON conformance harness **before** porting any loader.
5. Write the deterministic replay test scaffold (seed + input log → state hash).
