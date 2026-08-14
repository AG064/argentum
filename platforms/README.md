# Native platform targets

Argentum keeps platform behavior behind `argentum-platform` and uses Slint's
native backends for the desktop shell.

The product target matrix is Windows, macOS, Linux, Android, and iOS. This is a
target matrix, not a statement that every artifact currently exists or has been
verified. The current workspace contains the desktop Slint host and Windows
portable build and validation scripts.

Android is the first mobile design target. There is currently no Android host,
APK packaging, signing configuration, safe-area or IME adapter, or device test
evidence. iOS also has no application host or package in this workspace. The
mobile interaction and platform boundary is defined in
`docs/MOBILE_DESIGN_DRAFT.md`.
