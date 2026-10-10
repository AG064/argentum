# ChatGPT plan sign-in

This optional Windows route uses Argentum's own native OAuth registration and
OS credential vault, not a borrowed Codex credential or API key. It is separate
from paid OpenAI API access and from other providers. The official flow is a
preview for eligible open-source or locally hosted applications. Do not assume
that commercial/remote hosting, every account, or every model is eligible.
[Official eligibility](https://developers.openai.com/siwc/token-sharing-open-source).

In the owner's interactive PowerShell console. Run `provider add chatgpt` only
when creating a new profile; skip it for an existing configured profile so its
deliberate model/endpoint settings are not reset:

```powershell
$exe = 'C:\tools\argentum\argentum.exe'
$ws = 'C:\projects\example'
$db = 'C:\state\argentum\example.sqlite3'
& $exe provider add chatgpt --workspace $ws --database $db
& $exe provider login chatgpt --workspace $ws --database $db
& $exe provider models chatgpt --workspace $ws --database $db
& $exe provider model chatgpt --model EXACT_ACCOUNT_MODEL --workspace $ws --database $db
& $exe provider select chatgpt --workspace $ws --database $db
& $exe provider budget chatgpt --limits TOTAL,INPUT,MAX_OUTPUT,0,UNCERTAINTY --workspace $ws --database $db
& $exe status --workspace $ws --database $db --json
```

Complete consent only in the system browser. Do not export or paste bearer
tokens into commands, files, chat, screenshots or a provider-key field. Close
the application and run status again with the same paths to check persistence.
The catalogue contacts the account; it is not an inference request. Local
readiness does not establish live entitlement.

Registration starts with the official dynamic-client entrypoint, a stable
opaque per-host identifier, PKCE, state and nonce. The issued client identifier
and verified account binding are retained for reauthorization. The implementation
validates the signed identity and granted direct-plan permission before use.
It does not distribute another host's registration or credentials.
[Registration contract](https://developers.openai.com/siwc/token-sharing-open-source/sign-in).

The public Responses route uses `store: false`, `stream: true` and complete
request history. It does not use a ChatGPT private backend endpoint. Only a
completed terminal response establishes successful inference; failed,
incomplete and interrupted streams remain distinct.
[Inference contract](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference).

This preview disallows a provider-side output-token cap. Requested output zero
is therefore explicit uncapped output, not zero output. The owner-declared
maximum still reserves admission/accounting capacity, while byte/time/tool and
cancellation bounds remain. Choose values for the exact available model; no
example here supplies independently verified account limits.
[Preview restrictions](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations).

`provider accounts chatgpt` shows redacted saved account handles.
`provider account chatgpt ACCOUNT_ID` selects a registration.
`provider login-new chatgpt` adds one within the current bounded two-registration
implementation. Account/provider changes pause retained work and are rejected
during active execution. Tokens stay in versioned OS secure records, not SQLite.
Opaque continuation is bounded, run-local, account/model-bound and excluded
from durable history and reports.

`provider logout chatgpt` attempts issuer revocation first. A revocation/network
failure retains local credentials and reports failure rather than claiming
remote logout. Successful logout removes bearer state while preserving the
registration/host identity. Unsupported platforms fail closed.

RELEASE01 inspects this supported contract and runs synthetic controls only.
No new sign-in, catalogue or inference is part of release qualification.
