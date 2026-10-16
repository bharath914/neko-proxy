# Phase 2: rules, breakpoints, replay and AI access (MCP)

Linux only, on top of Phase 1. Phone pairing and the mobile SDK (Phase 3, `docs/sdk.md`) are not started.

## Ships

### Rules (`rules` crate, `rules.json` in the data dir)
One rule is a **match** (scheme, host pattern, port, method, path or `prefix*`, exact query) and an **action**:

| Action | Effect |
|---|---|
| `mock` | Answer with a fixed status, headers and body, optional delay. Server never contacted. |
| `map_remote` | Send to another scheme, host or port. The flow still shows the URL the client asked for. |
| `rewrite` | `set`, `add` or `remove` headers on the request, the response, or both. |
| `throttle` | Add latency before the request goes upstream. |
| `breakpoint` | Hold the request, the response, or both until resumed (gives up after `timeout_s`, default 60). |

Map Local and SSL proxying from Phase 1 are unchanged. Order of effect on a request: breakpoint (request) → rewrites, remap, delay → mock or Map Local → upstream → rewrites (response) → breakpoint (response).

### Breakpoints
- Held messages show with their body. Compressed bodies are decompressed for editing and `content-encoding` is dropped.
- Resume with edits to method, path and query, status, headers (replace) and body, or abort.
- Bodies above the capture cap (8 MiB) cannot be held: the client gets 413 and the flow fails with a reason.
- A forgotten breakpoint times out and lets the message through; a client that hangs up frees its slot.

### Replay
`replay(id, edits)` sends a stored flow again as a new flow (`app` = `proxy`, flag 1024). It goes straight upstream: rules and breakpoints do not apply.

### MCP server (`mcp` crate, in the core)
- Streamable HTTP, JSON responses, `POST /mcp` on `127.0.0.1:8898` (`--mcp-listen`, `--no-mcp`).
- `Authorization: Bearer <token>`; token in `<data dir>/mcp.token` (0600). Requests with an `Origin` header or a non-loopback `Host` are refused.
- Read tools (always on): `status`, `list_flows`, `search_flows`, `get_flow`, `diff_flows`, `export_har`, `list_rules`, `list_breakpoints`.
- Change tools: `create_mock`, `create_map_local`, `create_map_remote`, `create_rewrite`, `create_throttle`, `add_breakpoint`, `set_rule_enabled`, `remove_rule`, `resume_breakpoint`, `replay_request`, `start_capture`, `stop_capture`.
- Policy (Settings > AI clients, stored in `rules.json`): **Ask me** (default; each change shows an Allow/Deny card in the app and waits 2 minutes), **Allow**, or **Read only**.
- Redaction happens in the core: Authorization, cookies, API keys, token-shaped values and secret-looking JSON keys and query values. Results carry a note that captured content is untrusted data.

Connect Claude Code:

```bash
proxyd mcp        # prints the endpoint and the exact command
claude mcp add --transport http proxy http://127.0.0.1:8898/mcp --header "Authorization: Bearer <token>"
```

The same command is in the desktop app under Settings > AI clients.

### API additions (`proto/api.proto`)
`put_rule`, `set_rule_enabled`, `remove_rule` (any kind), `paused`, `resume`, `replay`, `watch` (pushes `PauseNotice` and `ApprovalNotice`), `approvals`, `decide_approval`, `set_mcp_writes`, `mcp_info`. `RulesResponse` now carries `rules` and `mcp_writes`.

### Desktop
Breakpoints screen (list and editor, count in the nav), Rules screen (create, edit, toggle, delete), approval card, AI clients group in Settings, and Replay / Mock / Break actions on a selected flow.

## Not in Phase 2
Bandwidth throttling (only latency), rules from YAML and hot reload from files edited by hand while running, scripting, WebSocket frame breakpoints, MCP over stdio, per-client tokens.
