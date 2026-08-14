# Router Agent decision layer

Router Agent evaluates message metadata against ordered rules and returns a target agent and optional
workspace. In v0.0.9, Argentum records that decision before continuing through its existing single-agent
dispatch path.

It does **not** yet create separate agent sessions, forward messages to different agent instances, or
enforce workspace-per-user isolation. Those dispatch and isolation capabilities are planned for v0.1.0.

## Configuration

The loader checks `config/router.yaml` and then `config/router.json`. YAML uses a top-level configuration:

```yaml
defaultAgent: agx
idMappings:
  ADMIN:
    numericId: '123456789'
rules:
  - condition: sender_id
    value: ADMIN
    targetAgent: admin
  - condition: keyword
    value: [admin, /admin]
    targetAgent: admin
  - condition: always
    value: ''
    targetAgent: agx
```

JSON may use the same top-level shape or the nested shape shipped in `config/router.example.json`:

```json
{
  "router": {
    "enabled": true,
    "defaultAgent": "agx",
    "rules": [
      {
        "condition": "chat_type",
        "value": "group",
        "targetAgent": "group-handler"
      },
      { "condition": "always", "value": "", "targetAgent": "agx" }
    ]
  }
}
```

Invalid configuration is rejected as a whole and Argentum falls back to empty rules with the `agx`
default. This keeps the single-agent runtime available without applying partially valid rules.

## Conditions

| Condition   | Match behavior                                                                        |
| ----------- | ------------------------------------------------------------------------------------- |
| `sender_id` | Exact ID match after prefix or friendly-name normalization                            |
| `chat_id`   | Exact chat ID match after normalization                                               |
| `chat_type` | `direct`, `group`, or `channel`                                                       |
| `keyword`   | Case-insensitive string or string-array match; `RegExp` is supported programmatically |
| `always`    | Unconditional fallback; place it last                                                 |

Friendly names resolve through `idMappings`. No production user or chat IDs are hardcoded in the router.

## Runtime boundary in v0.0.9

```text
message -> validate context -> evaluate rules -> record target decision -> existing Argentum agent
```

The `RouteResult` fields are reliable decision metadata. `sessionKey` remains empty because session
creation is not implemented. Do not treat a target agent name as proof that a different agent handled
the message.

## v0.1.0 work

- Resolve target names to registered agent instances.
- Create and reuse target-specific sessions.
- Enforce and test workspace isolation for multiple users.
- Define safe fallback behavior when a configured target is unavailable.
- Add end-to-end Telegram tests with at least two users.
