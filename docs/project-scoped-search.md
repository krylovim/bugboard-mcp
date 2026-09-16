# Project-scoped search: fork issue #1

Changes by krylovim, 2026-09-17. Apache-2.0 + Commons Clause 1.0; see LICENSE.

## Scope and branch

Implementation belongs to `codex/issue-1-project-scoped-search`, based on
`e5b9ff289881f9fed3378955205d46f0cbca24cd`. At the initial check, upstream
PR #2 was OPEN; upstream and fork main were both `48de9af`. The search fix is
inherited once. This feature does not change the old PR's branch.

## Contract for #2 and #3

- Persist `project_code`, never `project_handle` or an internal RPC reference.
  Product records have `project_code`, `title`, `abbreviation`, `updated_at`
  and the current MCP session's `project_handle`. `updated_at` comes from
  Bugboard, not from the time a cache was refreshed.
- `project_list({query, limit})` returns candidate records. Query is a literal
  substring over code OR official title OR abbreviation. Omit query to list
  the first page. `selection: "candidates_only"` forbids inferring that the
  first match is an automatically chosen product.
- `bug_search` resolves a supplied exact code on the server, independently of
  any limited catalog page. Codes use their returned spelling/case. Unknown
  codes yield `unknown_project`; duplicate codes yield `ambiguous_project`
  with limited safe candidates. Conflicting code/handle pairs yield
  `invalid_arguments`; unrecognized handles yield `invalid_reference`.
- A future cache can replace the exact catalog read behind `search_project`.
  It must retain the same unknown/ambiguous behavior and distinguish complete
  catalog loads from pages. No file cache, TTL, account-profile mapping or
  stale fallback is implemented by #1.
- The future skill determines the product from user intent and repository
  metadata and passes a code on **each call**. This MCP neither reads a
  repository nor writes `.bugboard.json`. Its format remains a #3 decision.
- Multiple tasks can search BP and ERP concurrently. All scope is request
  local. Handles belong to the server session that created them; they must
  not be copied to another session, where the same handle spelling can refer
  to a different object. Product codes are the cross-session contract.

## Search and completeness

The request contains the non-deleted predicate, project equality and either
number equality or title/description OR substring predicates in an AND group.
Search values are typed JSON strings, not interpolated expressions. The date
used for sorting must also be selected by the dynamic list. A reference is a
secondary sort key; this does not establish a supported pagination protocol.

The server requests `limit + 1` and enriches only the returned `limit` cards.
Project and number identity from the list is checked before truncation. A
response outside the requested scope raises `bugboard_changed`; it is never
silently filtered locally. Product identity and URL come from the list
snapshot, and other detail fields from subsequent card reads. No transactional
snapshot is promised.

`has_more` reports observed lookahead, and `coverage.more_results` is `yes`
or `unknown`. `coverage.complete` is always false, `total` null. No total count,
stable page boundaries, exhaustive index coverage or pagination is claimed.
For legacy RPC results, only that RPC's returned references are known.
`ambiguous: false` means multiple number candidates were not observed; it is
not a statement about inaccessible/deleted bugs or historical uniqueness.

`bug_get`, `bug_get_history`, `bug_get_subscription_vote_state` and
`bug_open_in_browser` keep their existing selectors. An ambiguous number
returns `ambiguous_bug_number` with up to two candidate cards and coverage;
use `bug_search` with a project, then a session handle to read a specific card.
Writes remain handle-only and are not exercised by acceptance tests.

## Validation

Automated coverage includes server-filter-before-limit with 60 unrelated rows,
description-only matches, product candidates, exact unknown/duplicate codes,
conflicting selectors, duplicate numbers even at limit 1, handle reads,
literal special characters, Cyrillic case, empty results, validation limits,
legacy calls, concurrent scopes, independent sessions and fail-closed decoding.
Wire tests check typed values and AND/OR/equality grouping.

Run the usual Rust quality gate from `.mise.toml`. For optional live read-only
acceptance, set `BUGBOARD_SESSION_ENV` to an existing external file, build the
server, then run:

```powershell
node scripts/search-live.cjs target/debug/bugboard-mcp.exe
```

The script starts and stops a separate stdio process. It does not modify any
installed MCP connection, read the cookie into the script, or persist secrets.
It uses BP `bp3`, ERP `erp2` and known card fixtures, so changing upstream data
can require reviewing fixture expectations. Results are printed as sanitized
check summaries. The production runtime and old session folder are untouched.
