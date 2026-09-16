# Issue #1 verification

Changes and checks by krylovim, 2026-09-17 (Europe/Moscow).

## Repository state

- `origin`: `https://github.com/krylovim/bugboard-mcp.git`.
- `upstream`: `https://github.com/bapho-bush/bugboard-mcp.git`.
- Feature: `codex/issue-1-project-scoped-search`, based on `e5b9ff2`.
- Upstream PR #2 remains OPEN, not merged; both main branches are `48de9af`.
  The old `fix/issue-1-full-text-search` remote branch remains `e5b9ff2`.
- All six fork issues were read. Only #1 is implemented; #2/#3 have a
  documented interface boundary, not a cache or diagnostic skill implementation.

## Automated checks

Rust 1.96.0, Windows GNU toolchain. Both `RUSTFLAGS` and `RUSTDOCFLAGS` used
`-C link-self-contained=yes` for this machine's portable MinGW environment.
The repository toolchain/check policy was not weakened or changed.

| Check | Result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo check --locked` | PASS |
| `cargo check --locked --examples` | PASS |
| `cargo clippy --locked -- -D warnings` | PASS |
| `cargo test --locked --workspace --all-targets` | 96 passed |
| `cargo test --locked --doc` | 1 passed |
| `cargo build --locked` | PASS |
| `cargo package --locked --manifest-path crates/e1c-element-rpc/Cargo.toml --allow-dirty` | PASS, package verified |
| `cargo deny check -A license-not-encountered` | PASS: advisories, bans, licenses, sources |
| `node --check scripts/search-live.cjs` | PASS |
| `git diff --check` | PASS |

The first advisory check rejected the inherited, yanked `chacha20 0.10.1`.
Only that transitive lock entry was upgraded to compatible `0.10.2`; no other
dependency version or manifest changed. The full gate then passed. Cargo-deny
0.19.9 came from its official release and was SHA-256 verified. Its temporary
config differs only in the advisory database path, keeping downloaded tooling
and caches in ignored `target/quality-tools`, outside the old runtime folder.
Non-fatal existing warnings remain: duplicate `getrandom`/`r-efi` versions and
missing optional publication metadata in the wire crate manifest.

## Live read-only acceptance

`scripts/search-live.cjs` exercises the real stdio MCP boundary using the
existing external session file. It does not change the installed connection.
The final binary passed all 9 acceptance groups, after the lockfile update.

- Exact product resolution: BP `bp3`, official ERP `erp2`. A broad
  «Бухгалтерия» query returns multiple candidates without choosing one.
- Concurrent `НДС` searches returned only the requested product. BP samples:
  `60010210`, `60009991`, `60009659`; ERP samples: `00-00054363`,
  `00-00155245`, `00-00052335`.
- BP card `10144553` has `НДС` in its description but not its title. A
  description substring search returned the card with description evidence.
- Limits 1 and 50 are applied after project/text filtering. At 50, all 50
  returned cards belong to BP and the extra matching row yields `has_more`.
- Unknown code, invalid handle and conflicting code/handle are rejected.
- Nonsense and special-character nonsense queries return valid empty results.
  `%`, `_`, quotes, backslash and `[` match literal characters in returned
  cards; Cyrillic lowercase `ндс` works. These observations do not promise
  identical collation for every language or Unicode character.
- `00-00843958` produces candidates in `erp2`, `trade11`, `ara20`.
  Unscoped `bug_get` returns `ambiguous_bug_number`; scoped ERP search is
  unambiguous and opening its handle returns the correct card.
- Existing text, empty text, numeric `60021238`, number-based `bug_get` and
  argument-compatible `project_list` calls work.

Transient HTTPS timeouts occurred during some separate runs before auth;
these were transport failures, not silently converted to empty results.
The acceptance script is intentionally explicit about such failures.

## Preservation and remaining work

The original LICENSE (Apache-2.0 + Commons Clause 1.0, Konstantin Redkin)
is byte-for-byte unchanged. Modified source files identify fork changes.
No session file, credentials, runtime binary or generated build output is
part of this change.

The installed binary in the prior task's `outputs/runtime` still has SHA-256
`89342A483B28F1B3D8DEF2EE823065EF8F81A75FE3E5157FBE9B8BF3F999A48D`.
The tested replacement in this project's `target/debug` has SHA-256
`27138FDC32D415776C726F25FA6EEAB819A27A6153F4E4405181CF3B819EEE92`.

Review/merge and a separate prepared runtime replacement remain. The running
MCP connection is unchanged, so active tasks do not yet expose these new
parameters. Pagination, exhaustive catalog loading, #2 cache, #3 repository
binding/skill and #4–#6 authorization/packaging are outside this implementation.
