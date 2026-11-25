# Linux and macOS (coming)

There are no release files for Linux or macOS yet, and the desktop app has not been
tested on them.

The command-line agent, Argentum Genesis, builds from source on both. It is not
tested for release there. To try it, install Rust, then from a clone of the
repository:

```sh
cargo run -p argentum-cli --bin argentum -- --help
```

Then follow [the command line](../cli.md). Reports about what works and what does
not are welcome as
[GitHub issues](https://github.com/AG064/argentum/issues).
