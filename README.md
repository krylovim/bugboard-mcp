# bugboard-mcp

Fork changes by krylovim (2026): restored legacy search and added explicit
product-scoped content search. Original license and author notices are retained.

`bugboard-mcp` gives an MCP client access to projects, versions, bugs, history,
subscriptions, and votes in the 1C Bugboard. It uses the browser session you
already have; it does not collect credentials or automate the browser.

The server is unofficial and experimental. Use it with an account that is
allowed to access the target Bugboard data.

## What it does

- Lists projects, versions, recent and subscribed bugs, and bugs you voted for.
- Searches by bug number or full-text query, then returns bug details and
  reference-redacted history.
- Creates session-local opaque handles so MCP clients never receive Bugboard
  references.
- Subscribes, unsubscribes, votes, and unvotes safely: every write checks the
  current state, skips a no-op, and confirms the result.

The default transport is Streamable HTTP at `http://127.0.0.1:8000/mcp`. For
local integrations and smoke checks, use stdio instead.

## Run it

Install the pinned toolchain:

```sh
mise install
```

Set `BUGBOARD_COOKIE` directly, or create an env-file outside the repository:

```text
BUGBOARD_COOKIE=...
```

Start the server with a direct value:

```powershell
$env:BUGBOARD_COOKIE="..."
mise run run
```

Or point it to an env-file:

```powershell
$env:BUGBOARD_SESSION_ENV="C:\path\outside\repo\bugboard.env"
mise run run
```

The server uses a direct `BUGBOARD_COOKIE` in preference to the env-file. It
loads only `BUGBOARD_COOKIE` from that file. It obtains the
deployment-specific `X-G5-Version` from the authenticated Bugboard shell and
keeps it in memory. Replace an expired cookie and restart the server. A read
may retry once after a Bugboard deployment change; a write never retries on its
own.

Use `mise run run:stdio` to use stdio. `BUGBOARD_MCP_BIND` can change the
address or port.

## Tool inputs

List and search tools return opaque handles. Handles work only in the MCP
session that created them. `bug_get*` and `bug_open_in_browser` take either a
`bug_handle` or a `bug_number`; write tools require `bug_handle`, and vote
tools also require `vote_kind`. `project_get_versions`, `project_subscribe`,
and `project_unsubscribe` require `project_handle`. `version_get_bugs` needs a
project handle and the exact version title returned by `project_get_versions`.

`bug_list_recent` returns the first page. List/search limits range from 1 to 50.
`project_list` accepts optional `query` to find products by literal substring
of their official title, abbreviation or code. It returns candidates with a
stable `project_code` and a temporary `project_handle`. A broad query such as
«Бухгалтерия» must not be resolved by selecting its first result.

For content search in a known product, call `bug_search` with:

```json
{"query":"НДС","project_code":"bp3","mode":"text","limit":20}
```

The server applies `project AND (title CONTAINS query OR description CONTAINS
query)` **before** limiting results. `project_handle` may be used instead of
`project_code`; if both are supplied they must identify the same product.
Unknown or ambiguous product codes fail explicitly. Each call supplies its
own scope; there is no mutable current project shared between tasks.

Modes and compatibility:

| Input | Search behavior |
| --- | --- |
| `mode: "text"` | Literal title/description substring, including digit-only text |
| `mode: "number"` | Exact number; ASCII digit groups with optional single hyphens |
| `mode: "auto"` | Detect a number, otherwise use title/description substring |
| Project supplied, mode omitted | Same as `auto`, scoped to that product |
| No project or mode, digit/hyphen number | Exact number, across visible products |
| No project or mode, other text | Original full-text RPC, for compatibility |

Old `query`/`limit` calls and result fields remain supported. Exact-number
search now returns candidates and `ambiguous: true` when it observes multiple
cards, even with `limit: 1`. `bug_get` and the other number-based read tools
reject ambiguous numbers with `ambiguous_bug_number`; use a candidate's
`bug_handle`, or search again with `project_code`. A number is not a globally
unique bug identifier.

Search results include product identity, number, URL and the applied `filter`.
`filter.mode` retains `bug_lookup`/`full_text` for old consumers;
`filter.effective_mode` states the actual `number`, `text` or
`legacy_full_text` operation. Text results include `matched_fields`, a local
case-insensitive check against the list snapshot; server collation may differ.

`has_more: true` means an extra matching row was observed by requesting
`limit + 1`; `false` means no extra row was observed, **not** proof of complete
results. `coverage.complete` is conservatively false, `coverage.total` is null,
and cursor pagination is unsupported. The first page, upstream access rules,
server caps and changing data limit what can be concluded. Literal search is
not semantic or morphological search. An empty result does not prove that no
known bug exists, and a fix version does not establish affected versions.

See [the API and acceptance notes](docs/project-scoped-search.md) for the
boundary with future catalog caching (#2) and the diagnostic skill (#3).

## Development

Run the complete local and CI check:

```sh
mise run verify
```

The workflow in `.github/workflows/ci.yml` runs the same command on Ubuntu and
Windows. The wire-level crate is documented in
[`crates/e1c-element-rpc/README.md`](crates/e1c-element-rpc/README.md).

## Docker

The image runs Streamable HTTP by default on port 8000. Publish it only to the
host loopback interface, because the MCP endpoint has no separate client
authentication. Build it with:

```sh
mise run docker:build
```

Pushes to `main` publish `ghcr.io/bapho-bush/bugboard-mcp:latest` and an
immutable Git commit tag. While the repository is private, pull it with a
GitHub token that has `read:packages`:

```sh
docker pull ghcr.io/bapho-bush/bugboard-mcp:latest
```

Run HTTP MCP for Codex:

```powershell
docker run --rm -d --name bugboard-mcp `
  -p 127.0.0.1:18080:8000 `
  -e "BUGBOARD_COOKIE=$env:BUGBOARD_COOKIE" `
  ghcr.io/bapho-bush/bugboard-mcp:latest
```

Configure Codex with `url = "http://127.0.0.1:18080/mcp"`. Do not use
`-p 18080:8000`, which publishes the unauthenticated MCP endpoint beyond the
local machine.

Run stdio instead by overriding the transport:

```powershell
docker run --rm -i `
  -e "BUGBOARD_MCP_TRANSPORT=stdio" `
  -e "BUGBOARD_COOKIE=$env:BUGBOARD_COOKIE" `
  ghcr.io/bapho-bush/bugboard-mcp:latest
```

An external session file remains supported for either transport when mounting
it is preferable:

```powershell
docker run --rm -d --name bugboard-mcp `
  -p 127.0.0.1:18080:8000 `
  --mount "type=bind,src=C:\path\outside\repo\bugboard.env,dst=/run/secrets/bugboard.env,readonly" `
  -e BUGBOARD_SESSION_ENV=/run/secrets/bugboard.env `
  ghcr.io/bapho-bush/bugboard-mcp:latest
```

## Security

Keep cookies, tokens, browser profiles, and authenticated request bodies out
of the repository. Publish a Docker HTTP port only to host loopback. Browser
requests with an `Origin` header must use an allowed origin.

## License

This project is source-available under [Apache-2.0 with Commons Clause
1.0](LICENSE). You may use, modify, and redistribute it, including for internal
commercial work. You may not sell the software itself or a product or service
whose value substantially comes from its functionality. It is not an
OSI-approved open-source license.
