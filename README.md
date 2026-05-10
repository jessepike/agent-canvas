# Vellum

Live Markdown. Plain files. Honest history.

Vellum is a desktop Markdown editor where executable blocks can refresh data inline while the file remains ordinary Markdown. The project is built around a byte-level source-preservation contract: opening and saving an unchanged document must not pretty-print, normalize, or otherwise rewrite the source.

The canonical design source is [`vellum-spec-v0.3.md`](vellum-spec-v0.3.md). The contributor rules and format-preservation philosophy are in [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Build

Rust builds run from the workspace root:

```sh
cargo build --workspace
cargo test --workspace
cargo run -p vellum-corpus
```

The UI is intentionally not scaffolded yet. Gate 30A is parser and corpus only.

## License

Apache License 2.0. See [`LICENSE`](LICENSE).
