# Upstream PR #2: CI investigation, 2026-10-04

Investigation by krylovim; existing repository licensing applies.

## Conclusion

[PR #2](https://github.com/bapho-bush/bugboard-mcp/pull/2) was merged on
2026-10-02 at 12:49:07 UTC as `1629c9037d23255d47979a351711bd51340553fd`.
The failed checks are an inherited dependency-policy failure, not a failing
search regression or runner/network outage. `cargo deny` rejects the yanked
`chacha20 0.10.1` locked through `rmcp 1.8.0 → rand 0.10.1 → chacha20 0.10.1`.

## Evidence

| Run | Tested source | Result |
| --- | --- | --- |
| [PR run 35143108538](https://github.com/bapho-bush/bugboard-mcp/actions/runs/35143108538) | PR head `e5b9ff2`; checkout log shows synthetic merge `f853446` into base `48de9af` | Ubuntu and Windows fail at final `cargo deny`; Docker build succeeds |
| [Main run 37009000075](https://github.com/bapho-bush/bugboard-mcp/actions/runs/37009000075) | Merge `1629c90` | Same failure on both OSes; Docker build and GHCR publication succeed |

Both runs reached the last command of `mise run verify`:

```text
cargo deny check -A license-not-encountered
error[yanked]: detected yanked crate (try `cargo update -p chacha20`)
chacha20 0.10.1
advisories FAILED, bans ok, licenses ok, sources ok
```

The preceding formatting, compilation, examples, Clippy, tests, build and package
verification succeeded. Each OS reports 40 + 2 + 32 tests, one doctest and 13
example tests passing. Duplicate-dependency warnings and the Node 20 deprecation
annotation are not the failing condition.

`Cargo.lock` is byte-identical in the original base `48de9af`, PR head `e5b9ff2`
and merged upstream `1629c90`: Git blob
`ae961306e9c60dc4b0f025873f5119383079908f`. All contain `chacha20 0.10.1`.
The PR changes only `crates/e1c-element-rpc/src/bugboard.rs`; it changes none of
`Cargo.lock`, `Cargo.toml`, `deny.toml`, `.mise.toml` or the workflow. The unchanged
policy explicitly sets `yanked = "deny"`.

Sources: [original lockfile](https://github.com/bapho-bush/bugboard-mcp/blob/48de9af60622110a483cb0d2177a023d77ca7c91/Cargo.lock),
[merged lockfile](https://github.com/bapho-bush/bugboard-mcp/blob/1629c9037d23255d47979a351711bd51340553fd/Cargo.lock),
[merged policy](https://github.com/bapho-bush/bugboard-mcp/blob/1629c9037d23255d47979a351711bd51340553fd/deny.toml).
The [crates.io registry API](https://crates.io/api/v1/crates/chacha20), checked
2026-10-04, reports `0.10.1: yanked=true`, `0.10.2: yanked=false`.
These observations do not establish the reason for yanking or claim a CVE.

## Required action and fork status

Upstream should update this locked dependency, for example with
`cargo update -p chacha20 --precise 0.10.2`, review the lockfile diff, then rerun
`mise run verify`. Disabling the dependency policy or reverting the search fix
does not address the cause. The main workflow currently publishes its image
independently of the verification matrix; that explains publication despite
the failed policy check.

Our fork's `47715a5` already locks `chacha20 0.10.2`. This change was introduced
in [db9ac11](https://github.com/krylovim/bugboard-mcp/commit/db9ac110eb8e40b499ddfcf508d0065052b15659)
on 2026-09-17. No duplicate dependency fix is needed in the fork for this error.

Verification here: read all four failing job logs, compared Git objects and
policy files, and queried the registry. No upstream source, comments, issues,
branches or runs were modified. No new full local build or current fork CI run
was performed as part of this investigation; the conclusions concern these
specific historical failures.
