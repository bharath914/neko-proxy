# Composer, Postman-style (plan)

Goal: the Composer should feel familiar to anyone who uses Postman, in Neko's own look (tokens in `Theme.swift`, Inter + JetBrains Mono, rounded cards, restraint). Layout and vocabulary follow Postman; colors, type, motion and materials stay ours. Logic stays in the core (`core/crates/compose`, `core/crates/api/src/composer.rs`); the Swift app only edits fields and draws.

## Layout

```
┌ sidebar ─────────┬ request tabs  [GET /users ●] [POST /login] [+]        [env ▾] [👁] ┐
│ Collections      ├───────────────────────────────────────────────────────────────────┤
│ Environments     │ Collection / Folder / Request name                  [Save] [</>]   │
│ History          │ [GET ▾] [ https://{{host}}/users?page=1        ] [Send ▾]         │
│ [search…]  [+]   │ Params  Authorization  Headers (3)  Body  Tests  Settings          │
│ ▾ Shop           │ ┌ editor ─────────────────────────────────────────────────────┐   │
│   ▾ Orders       │ └─────────────────────────────────────────────────────────────┘   │
│     GET list     ├── response ───────────────────────────────────────────────────────┤
│     POST create  │ Body  Cookies  Headers (9)  Tests (2/3)    200 OK · 84 ms · 1.2 KB │
│ ▸ Auth           │ Pretty Raw Preview  JSON ▾   wrap  search  copy  save              │
└──────────────────┴───────────────────────────────────────────────────────────────────┘
```

## Feature list (maps to Postman)

Workspace
- Open-request tabs with method color, name, unsaved dot, close, middle-click close, `+`, ⌘T new, ⌘W close, ⌃Tab cycle. Tabs are not persisted across launches in this pass.
- Sidebar tabs: Collections, Environments, History; search filter; nested folders (collection paths use `/`); drag to reorder is later.
- Environment picker top right with a quick-look popover (current values, globals).

Request
- Method menu with all standard methods plus custom; URL bar with `{{variable}}` highlighting; paste cURL to import.
- Params: query table and path variables (`:id`) kept in sync with the URL both ways; description column; bulk edit.
- Authorization: Inherit from parent, No Auth, API Key (header or query), Bearer (prefix), Basic.
- Headers: table with description, bulk edit, a read-only "auto-generated" list (User-Agent, Accept, Content-Type, Authorization as sent).
- Body: none, form-data (text fields), x-www-form-urlencoded, raw (Text, JSON, XML, HTML, JavaScript), binary (file), GraphQL (query + variables). JSON beautify.
- Tests: declarative assertions (status, header, body contains, JSON path equals/exists, response time under), run after every send, results in the response pane. No scripting engine (scripting is a later milestone).
- Settings: follow redirects, request timeout, verify TLS stays on (the proxy owns TLS).
- Collection and folder settings: auth, headers and variables inherited by their requests.
- Variables: global, collection, environment (secret values masked), dynamic (`$guid`, `$timestamp`, `$randomInt`, more).
- Code snippets: cURL, fetch, Python requests, Swift URLSession, Kotlin OkHttp, HTTPie, Go; generated in the core.

Response
- Status chip, time, size; tabs Body, Cookies, Headers, Tests, Timing; Pretty/Raw/Preview, wrap, search in body, copy, save to file; "Show in Inspector".

Import and export
- cURL (existing), Postman Collection v2.1 import/export, environment import/export.

## Core changes

- `compose::Request` gains: `description`, `path_vars`, `form_data` kind, `raw_kind`, `graphql_variables`, `auth` variants (inherit, api key), `tests`, `settings`.
- `compose::State` gains `globals`, `collection_configs` (path → auth, headers, vars, description), `folders` implied by `/` in collection paths.
- New RPCs: url sync (`ComposerUrlSync`), bulk edit parse/format, code generation, Postman import/export, run tests (tests run in the core against the stored flow).
- Everything stays in `composer.json` (backward compatible: new fields default).

## Order of work

1. Core model and builder (rust, with tests).
2. Proto, API handlers, regenerate Swift bindings.
3. Swift: tab bar and workspace model, sidebar, request editor, response pane, dialogs.
4. Screenshots through `Shots.swift`, docs, CLAUDE.md note.
