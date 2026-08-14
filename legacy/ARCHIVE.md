# Legacy archive boundary

This directory contains the previous application tree preserved during the
native Rust rewrite. It is archival and is not a dependency of the active
Cargo workspace.

The active application has no runtime, build, or UI imports from this folder.
Do not add new product code here. Use the root Cargo workspace, `crates/`, and
`ui/` for the native application.
