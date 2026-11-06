# Voice mode design

Status: design only. Nothing in this document is built yet.

## Goal

Talk to Argentum instead of typing, and optionally hear answers read aloud. Voice
ships with the default build, but it is off until the owner turns it on in
Settings.

## Speaking to Argentum

- A microphone button in the composer, and a push-to-talk key. Holding the key
  records; releasing it stops.
- Speech becomes text in the composer. By default, the owner reads and edits it,
  then sends it. An optional hands-free mode sends after a pause, and it is off by
  default.
- A recording stops after 2 minutes.
- While the microphone is open, the composer shows a clear recording indicator.

## Speech to text

Two choices, set in Settings > Voice:

| Choice | Where audio goes | Notes |
|---|---|---|
| Local (default) | Stays on the machine | whisper.cpp through `whisper-rs`. The owner downloads a model in Settings; Argentum never downloads one on its own. Small models run on a CPU. |
| Provider | To that provider's address | An OpenAI-compatible `/audio/transcriptions` endpoint on a configured provider. Sending audio is a network action and needs approval, like any other. |

Audio is captured with `cpal`, kept in memory, and dropped after transcription.
Argentum does not save recordings, unless the owner turns on keeping them for
debugging.

## Answers read aloud

- Optional, and off by default.
- Local text to speech: the Windows speech API on Windows, and `speech-dispatcher`
  on Linux.
- Only the answer is read, not tool output, diffs or code blocks. Code is replaced
  by a short phrase, such as "code block, 12 lines".
- Esc or the push-to-talk key stops reading at once.

## Voice commands

Spoken text is a prompt, like typed text, with the same approval policy. A few
words are recognized as app actions, only when the whole phrase matches: "stop",
"send", "cancel", and "read that again". Anything else is sent to the model, and
nothing spoken can approve a write, a shell command or a network request. Those
still need a click or a key press.

## Security and privacy

- The microphone is used only while the owner records. Argentum never listens in
  the background or for a wake word.
- The first use asks for Windows microphone access through the system prompt.
- Transcripts are prompts, so they are stored like typed prompts. Audio is not.
- Provider transcription shows the provider's address before the first use.

## Phases

1. Push-to-talk, local transcription, editable text in the composer. Windows.
2. Provider transcription.
3. Answers read aloud.
4. Hands-free mode and the short voice command list.
5. Linux.

## Open questions

- Which default local model size gives good accuracy on a typical CPU.
- Whether voice should be a separate download to keep the installer small.
