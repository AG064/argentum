# ChatGPT plan sign-in

This optional Windows route lets you use a ChatGPT plan through Argentum's own sign-in. It
uses its own registration and stores tokens in the Windows credential store. It is not an
API key, and it is separate from paid OpenAI API access and from other providers. OpenAI
offers the sign-in flow as a preview for open-source and locally hosted applications. Not
every account or model is eligible, and commercial or remote hosting may not be.
[Official eligibility](https://developers.openai.com/siwc/token-sharing-open-source).

Run these commands in an interactive PowerShell window. Run `provider add chatgpt` only
when you create a new profile. Skip it for a profile you already set up, so your model and
endpoint choices stay as they are.

```powershell
$exe = 'C:\path\to\argentum.exe'
$ws = 'C:\path\to\project'
$db = 'C:\path\to\argentum.sqlite3'
& $exe provider add chatgpt --workspace $ws --database $db
& $exe provider login chatgpt --workspace $ws --database $db
& $exe provider models chatgpt --workspace $ws --database $db
& $exe provider model chatgpt --model EXACT_ACCOUNT_MODEL --workspace $ws --database $db
& $exe provider select chatgpt --workspace $ws --database $db
& $exe provider budget chatgpt --limits TOTAL,INPUT,MAX_OUTPUT,0,UNCERTAINTY --workspace $ws --database $db
& $exe status --workspace $ws --database $db --json
```

Finish the sign-in only in the browser window that opens. Never paste a token into a
command, file, chat or key field. To check that the sign-in was saved, close Argentum and
run `status` again with the same paths.

`provider models` asks your account for its model list. It does not run a model. `status`
checks only what is saved on this computer. It does not confirm that your plan can use a
given model.

## How the sign-in works

Argentum registers itself with OpenAI's sign-in service using PKCE, a state value and a
nonce, plus a stable random identifier for this computer. It keeps the client identifier
it receives and the account it was granted for, so it can sign in again later. Before using
a sign-in, it checks the signed identity and that your plan grants direct use. It does not
include anyone else's registration or credentials.
[Registration contract](https://developers.openai.com/siwc/token-sharing-open-source/sign-in).

Requests use the public Responses API with `store: false`, `stream: true` and the full
conversation history each time. They never use a private ChatGPT endpoint. Only a
completed response counts as a successful answer. A failed, incomplete or interrupted
stream is reported as such.
[Inference contract](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference).

The preview does not allow an output-token limit on the provider side. For that reason the
requested output in the budget command above is `0`, which means no cap on output, not
zero output. The maximum output you declare still sets how much budget Argentum reserves
for a request, and byte, time, tool and cancellation limits still apply. Choose values for
the exact model you use. The example does not know your account's real limits.
[Preview restrictions](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations).

## Accounts and sign-out

Argentum supports up to two ChatGPT sign-in accounts.

- `provider accounts chatgpt` lists saved accounts with redacted handles.
- `provider account chatgpt ACCOUNT_ID` selects an account.
- `provider login-new chatgpt` adds a second account.

Changing the account or provider pauses queued work, and it is refused while a run is
active. Tokens are kept in Windows secure storage, never in the SQLite database.

`provider logout chatgpt` asks OpenAI to revoke the sign-in first. If that fails, for
example because you are offline, Argentum keeps your local credentials and reports the
failure instead of claiming you signed out. When logout succeeds, the tokens are removed
and the registration for this computer is kept. On other platforms, sign-in is not
available.
