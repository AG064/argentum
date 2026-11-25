# Troubleshooting

## Windows warns when I run the installer

The Windows files are not code signed yet. In the SmartScreen dialog, choose
**More info**, then **Run anyway**. To check the file first, compare
`Get-FileHash FILE` with `SHA256SUMS.txt` from the same release.

## The window is blank

The app draws with the CPU by default. If you changed the renderer, or a graphics
driver misbehaves, set the `SLINT_BACKEND` environment variable to choose another
renderer, or remove it to go back to the default.

## "The local model add-on is not installed"

llama.cpp is selected, but the add-on is not installed. Install it in
Settings > Providers > Local model add-on, or pick another provider. See
[run a local model](local-models.md).

## Requests stop with `spend_limit_reached`

The provider reached the monthly token limit you set. Raise or clear the limit in
the Usage panel. See [compaction and budgets](compaction-and-budgets.md).

## The model says the context is full, or replies get worse in a long session

Compact the session: Settings > Diagnostics > Context > **Compact**, or `/compact`
in the terminal. Or start a new session. See
[compaction and budgets](compaction-and-budgets.md).

## A goal stopped

The goal card, or `argentum goal status --json`, names the reason, such as
`goal_budget_exhausted` or `goal_deadline_exceeded`. See [goals](goals.md).

## `approval_required` from `argentum serve`

The JSONL server cannot approve anything. Approve the action in the app or in
`argentum chat`. See [the JSONL server](jsonl-server.md).

## "Another copy is open"

Only one Argentum process can use a project's data at a time. Close the other copy.
If the app did not close cleanly, the next start recovers by itself once it has
proven the previous copy is gone. It records the interrupted work, retires its
approvals, pauses goals and replays nothing.

## On Android, the app opened its own folder instead of mine

All files access was turned off, or the folder was deleted. Turn All files access
back on in Android settings and choose the folder again under Settings > Workspace.

## Check the whole setup

From a terminal:

```powershell
argentum doctor --workspace $ws --database $db --json
```

It is read-only and contacts nothing. Each failed check says what is missing.

## Report a problem

1. Settings > About > **Export this session** writes the session as JSON, with
   saved keys and known secret values removed. Read it before you share it.
2. Open an [issue on GitHub](https://github.com/AG064/argentum/issues), or write to
   contact@ag064.eu.

Security problems go through the private process in the
[security policy](https://github.com/AG064/argentum/blob/development/SECURITY.md),
not a public issue.
