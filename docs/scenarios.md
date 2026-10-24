# Scenario rules

Linux Rules → New rule → Scenario exposes:
- fail_one_in: every Nth matching request returns 503 (0 disables).
- delay_ms + jitter_ms: base delay plus random 0..jitter_ms inclusive.
- flaky_one_in: random 1/N matching requests simulate an upstream failure (502).

Combined delay is at most 60 seconds. Flaky simulation uses reproducible
pseudo-random values; it does not corrupt TCP packets or interrupt an already
streaming response. Every-N counters are per rule and process, including only
matched requests. Changes do not interrupt requests already in flight.

Create `<data-dir>/rules.yaml` before startup to use YAML instead of rules.json:

```yaml
rules:
  - id: r100
    name: Intermittent checkout failure
    match: {host: api.example.com, path: /checkout}
    action:
      type: scenario
      fail_one_in: 3
      delay_ms: 200
      jitter_ms: 100
      flaky_one_in: 0
```

Schema: scenario.schema.json describes each scenario entry. Core validation
also enforces combined delay and a nonempty action. JSON remains supported.
The selected file is polled every 500 ms on a blocking worker. Invalid reloads
retain the last valid state. Files are limited to 8 MiB and rules to 1024.
API writes use atomic replacement; the file remains valid YAML even when saved
as JSON. Reopen/refresh Rules to see external edits in the UI.

MCP create_scenario accepts the four settings and ordinary match fields. It
uses the existing Ask/Allow/Read-only policy and applies nothing before approval.
Proto adds ScenarioAction and Rule.scenario = 15; regenerate Swift bindings
with desktop-macos/gen-proto.sh.

Validation: cargo test --workspace passed. Scenario tests verify frequency,
jitter, YAML reload and invalid-file retention. MCP tests verify approval and
read-only rejection. Screenshot run: 43 desktop tests passed; inspected
scenario-editor-light.png. Initial compile failed E0425 (rule limit check in
the wrong function); corrected before successful tests. Performance targets
have not been re-benchmarked.

YAML API reference used: https://docs.rs/serde_yaml_ng/0.10.0/serde_yaml_ng/fn.from_slice.html
