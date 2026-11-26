<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<!-- Copyright (C) 2026 AG064 -->
# Community model speeds

`local-model-speeds.json` lists generation speeds that users measured with
Argentum and chose to share. The model marketplace in Settings shows them next
to each quantization. A copy is built into the app, and "Refresh speeds"
fetches this file only when pressed.

## How an entry gets here

1. In Argentum, a user runs a local model and presses "Measure speed". The
   measurement runs on their machine and nothing is sent.
2. Argentum shows the exact report: repository, quantization, GPU model, RAM
   size in GB and tokens per second. Nothing else.
3. "Open report on GitHub" opens a prefilled issue in the user's own browser.
   The user submits it there, or closes it.
4. A maintainer adds the entry here by pull request, with `source` set to the
   issue address.

Argentum never sends a report itself. See the no telemetry rule in
`SECURITY.md`.

## Format

```json
{
  "schema": 1,
  "entries": [
    {
      "repo": "owner/Model-GGUF",
      "quant": "Q4_K_M",
      "gpu": "GPU name as Windows reports it",
      "ram_gb": 32,
      "tokens_per_second": 41.5,
      "source": "https://github.com/AG064/argentum/issues/123"
    }
  ]
}
```

The app refuses a table with unknown fields, an invalid repository name or
quantization, a speed outside 0 to 10,000 tokens per second, or a source
outside this repository.
