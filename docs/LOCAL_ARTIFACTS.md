# Local document artifacts

This optional Windows feature lets the model write a document in Markdown,
convert it to a PDF on your machine, and look at the rendered pages to check the
result. It needs no Google account or other connected service.

It is not part of any 0.1.0 download. You build it with the `artifacts` feature,
which also enables [extensions](EXTENSIONS_BROWSER.md). The model still talks to
your configured provider, so the Markdown and page images it sees are sent there.

## What the PDF supports

You edit UTF-8 Markdown. The PDF converter supports:

- headings 1 to 3, paragraphs, one-level lists, rules and literal code
- tables with at most 4 columns and 32 rows
- Latin, Greek and Cyrillic text with precomposed letters

It rejects raw HTML, links, images, bold and italic text, nested lists, templates
and other syntax. Missing glyphs, code lines wider than the page and oversized
documents fail with an error. It does not do Word or other office formats,
complex scripts, OCR or visual editing.

## Build and commands

```powershell
cargo build -p argentum-cli --features artifacts --bin argentum --release --locked
cargo build -p argentum-artifacts --bin argentum-pdf-worker --release --locked
& $exe artifacts help
& $exe artifacts tools
& $exe artifacts inspect --config $configuration
```

`scripts/package-windows-cli.ps1 -Artifacts -ArtifactAssets <dir>` builds a
portable package that includes the PDF worker, PDFium and the Noto Sans font.

- `artifacts serve --config <file>` is the tool server the model uses. Register
  and activate it as an [extension](EXTENSIONS_BROWSER.md) first.
- `artifacts verify --config <file>` checks a finished artifact. Register it as a
  project runner and use it as a goal criterion.
- `<file>` must be an absolute path to a configuration file kept outside the model's
  workspace. It names the two input files, the Markdown file, the output folder
  and the exact worker, font and PDFium files, each with a SHA-256. It holds no
  credentials. Changing it removes the activation.

The model gets three tools: `artifact_open` (shows the inputs, the current
Markdown hash and the selected artifact), `artifact_render` (creates a PDF from
the current Markdown) and `artifact_page` (prepares one page tile as an image).
It edits the Markdown with the ordinary file tools.

## Generations and storage

Each render creates a new artifact folder and keeps the earlier ones. At most
four generations are kept, and older ones are not deleted automatically. After
four, rendering fails until you remove one yourself. If the Markdown changed since
the model last looked, it must open the artifact again before rendering.

## Isolation

The PDF worker runs as a separate process with no network access and none of
your credentials, under the Windows low-privilege app container (LPAC) with a
512 MiB memory limit and a 30 second time limit. The bundled PDFium build has
JavaScript (V8) and XFA forms disabled. PDFs with forms, scripts, attachments
or encryption, and malformed PDFs, are rejected. This is not protection against
a compromised Windows or administrator account.

## Page images

Pages are rendered 1000 pixels wide and cut into PNG tiles at most 800 pixels
high and 128 KiB each. The ChatGPT route sends the PNG bytes to the model. Other
providers do not accept image input. After a new artifact is selected, earlier
page images are not sent again.

## Verification

`artifacts verify` checks that the Markdown, PDF and rendered pages still match
what was recorded, parses and renders the PDF again in a fresh process, compares
the pixels, checks the text tokens, and checks that every page tile was prepared.
It does not check that the text is correct, that table facts are right, that
nothing overlaps or is clipped, or that the model understood the pages. Review
the content and the rendered pages yourself.

Changing a source, the Markdown, the PDF, the font or the worker invalidates the
checks that depend on it. A goal finishes only when the approved runner has passed
on the current files and no work remains.
