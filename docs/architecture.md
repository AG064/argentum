# Native architecture

```text
trusted local owner path        task / JSONL client
         |                            |
 local authority handle        typed AppCommand
         +---------------+------------+
                         |
              argentum-cli CommandHost
                         |
                ComposableHarness
                         |
                 RuntimeService
                 /     |      \
           providers  tools   store
                 \     |      /
               security and platform authority
                         |
                      AppEvent
```

The owner path and untrusted task/protocol path are distinct. Serialized input,
model output and presentation state cannot manufacture an authority ticket.
Headless frontends disconnect the owner approval responder; a needed new
approval fails explicitly. Workspace policy, source identity and cancellation
are rechecked at admission and effects. Read-only inspection is separate from
writable ownership/migration/recovery.

`argentum-domain` defines typed state/contracts. `argentum-cli` owns executable
commands, terminal and JSONL. `argentum-runtime` owns request/context/goal/tool
orchestration. `argentum-providers` owns bounded protocols and secure bindings.
`argentum-store` persists history/accounting/receipts, not credentials.
`argentum-workspaces` supplies contained observations and edit preparation;
`argentum-changes` supplies journal/diff semantics. `argentum-tools` brokers
capabilities; `argentum-execution` owns bounded processes/jobs/verifiers and
the optional restricted supplemental-check boundary. `argentum-security` and
`argentum-platform` enforce policy/authority and OS boundaries.

`argentum-harness` stages capability composition around `argentum-lifecycle`'s
generation, retirement and recovery. Existing first-party lifecycle modules are
not an external plugin loader. Local instruction/skill text remains untrusted,
bounded, provenance-labelled request input, never authority.

Hosted model requests use explicit effective budget/account/endpoint bindings.
Tool schemas reflect enabled capabilities. Transient continuation and verifier
diagnostics are private bounded request state, not exported reasoning or
invented historical evidence. Unknown accounting and effects remain stops.

Native Slint and Android source reuse the shared command host but have separate
platform/visual/release gates. Historical Node browser/MCP/plugin modules are
not runtime dependencies. Future optional extensions must enter this same
governed authority/lifecycle/result system. See [HARNESS_MODULARITY.md](HARNESS_MODULARITY.md)
and [ROADMAP.md](../ROADMAP.md).
