# Native platform targets

Argentum keeps platform behavior behind `argentum-platform` and uses Slint's
native backends for the desktop shell.

The product target matrix is Windows, macOS, Linux, Android, and iOS. This is a
target matrix, not a statement that every artifact currently exists or has been
verified. The current workspace contains the desktop Slint host, Windows
portable build and validation scripts, and the Android native host and debug
Gradle project.

Android is the first mobile implementation target. It has a native activity,
safe-area and IME handling, Keystore and workspace bridges, debug APK packaging,
and optional app-private on-device inference. API 36 lifecycle evidence exists
for the Milestone 2 host. Milestone 3 inference runtime evidence and physical
ARM64 evidence remain required. Release signing is not configured. iOS has no
application host or package in this workspace.
