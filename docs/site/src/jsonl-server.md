# The JSONL server

`argentum serve` keeps one agent running and talks JSON Lines on standard input and
output: one JSON request per line in, one JSON message per line out. Use it to
drive Argentum from another program, such as an editor plugin or a script.

## Start it

```powershell
argentum serve --workspace $ws --database $db
```

## Send requests

Write one request per line:

```json
{"protocol_version":1,"request_id":"1","type":"command","command":{"kind":"list_sessions"}}
{"protocol_version":1,"request_id":"2","type":"command","command":{"kind":"submit_task","prompt":"Summarize README.md"}}
```

Responses carry the `request_id` you sent, so you can match them to requests.

## Errors

- A line that is not valid JSON gets an `invalid_json` error.
- Valid JSON with an unknown field or the wrong shape gets `invalid_request`.

Both are recoverable, and the server keeps reading.

## Approvals

`serve` has no auto-approve switch. An action that needs a new approval fails with
`approval_required`. Approve it in an interactive session, in the app or
`argentum chat`, and the policy applies from then on. Settings, keys and monthly
limits cannot be changed over JSONL.

## Keep it local

`serve` is not a network service and has no sign-in. Treat whatever is connected to
it as untrusted, and do not expose it over a network. To reach another device,
[pair it](pairing.md) instead.

For evaluations, `--isolated` ignores saved and environment provider credentials
and uses only the absolute `--workspace` and `--database` paths you give. It is not
a filesystem sandbox: the project policy still decides what the agent can touch.

The request and response types are defined in
[`crates/argentum-cli/src/protocol.rs`](https://github.com/AG064/argentum/blob/development/crates/argentum-cli/src/protocol.rs),
and the headless options in the
[command-line guide](https://github.com/AG064/argentum/blob/development/docs/CLI_GUIDE.md#headless-use-and-isolation).
