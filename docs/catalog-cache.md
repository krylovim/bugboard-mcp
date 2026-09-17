# Catalog cache

Changes by krylovim, 2026. License and copyright notices remain in LICENSE.

The MCP caches product discovery and exact stable-code resolution for 24 hours.
There is no mutable current product: every bug request continues to pass its own
`project_code` or session-local `project_handle`.

`project_list` accepts the existing `query` and `limit`, plus optional
`force_refresh: true` and `metadata_only: true`. The response adds `cache`: source (`server`, `memory`,
`disk`, `stale_cache`), `stale`, `age_seconds`, `updated_at_unix`, `ttl_seconds`,
`persistent`, `references_available`, and an optional storage/refresh warning.
The refresh applies to the requested selector. Existing result fields remain.

## Partial coverage

The dynamic-list cursor and server caps have not been established. A cached
first page is **always partial**, even if it contains fewer than the requested
51 rows. `coverage.complete` and `cache.complete` are false. The cache stores
separate pages for exact selectors, literal queries, and the unfiltered first
page; it never treats a miss in that first page as an unknown product. Unknown
exact codes trigger a fresh exact server request, including repeated misses.
Exact resolution checks up to two rows and rejects ambiguity as before.
Titles and abbreviations remain candidates, with no implicit alias expansion.

The displayed `limit` is applied after fetching/caching the server-filtered
candidate page. Changing it does not fetch the same page again. A successful
exact-code resolution in the same MCP instance also avoids another catalog
request until TTL expiry. Bug content is never cached here.

## Storage and account scope

Disk persistence requires `BUGBOARD_SESSION_STORE=dpapi`. `BUGBOARD_PROFILE`
consists of 1–64 ASCII letters, digits, underscores or hyphens (default `default`).
An opaque random generation is captured atomically with the protected cookie
and included in the cache namespace. Replacing credentials, even under the same
profile, creates a separate cache; an older running process can only write its
old generation. Legacy direct-cookie and env-file callers retain an isolated
in-memory cache, even with an explicit profile: a profile name alone cannot prove
that its account has not changed. The server URL is part of the path and is
validated inside the file. `BUGBOARD_CACHE_DIR` can override the root;
the default is `%LOCALAPPDATA%/bugboard-mcp/cache` on Windows, with XDG/HOME
cache-directory fallbacks on other systems.

Format v1 stores only selector pages, update timestamps, stable code, official
title, abbreviation and product update date. It contains no cookie, password,
internal reference or session handle. At most 256 pages / 8 MiB are retained.
Invalid JSON, unknown format, identity mismatch or invalid content is ignored
and fetched again. A failed refresh does not replace the previous good file.

Writers use an OS file lock, merge the latest committed pages under that lock,
then flush and atomically replace the file from a temporary file in the same
directory. A concurrent writer may skip persistence with `cache_write_failed`;
its successful live response is still returned and retained in memory. OS locks
are released on process exit, including crashes. Readers see complete old or
new files. Network requests are coalesced within one MCP instance; independent
processes may both fetch, but cannot publish a torn file.

## Offline behavior and session references

Internal project references exist only in memory. After a restart, fresh disk
metadata answers `project_list(metadata_only:true)` without networking. Its
`project_handle` is `null`; callers should use `project_code`. The default
`metadata_only:false` preserves compatibility by hydrating live handles on the
first call and failing on refresh failure, rather than supplying unusable handles.
Scoped bug search
hydrates an exact code's live reference once per process, then reuses it until
expiry. If a required metadata refresh fails, `project_list(metadata_only:true)` returns the saved metadata
with explicit stale status and age; its `project_handle` is `null` and
`references_available` is false. A handle from a previous MCP process must never
be reused. The diagnosis skill should use `project_code` and inspect freshness.

An offline catalog helps identify candidate products; it does not make bug
content accessible offline. Search resolution requiring a live reference fails
on refresh failure instead of fabricating one. Authentication and network
errors must not be described as an empty bug search.

## Verification

Unit fixtures cover repeated resolution without networking, exact TTL expiry,
forced refresh, unknown-code refresh, account/server isolation, malformed and
unsupported files, partial coverage, failure preserving the good snapshot,
reference-free serialization, memory request coalescing, and independently
locked concurrent writers. HTTP fixtures verify unchanged search scoping and
explicit refresh through the MCP implementation. Live behavior is verified
separately in the task checkpoint; these fixtures do not prove cursor semantics.
