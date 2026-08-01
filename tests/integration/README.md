# Upstream `just`'s test suite

This directory is [casey/just](https://github.com/casey/just) 1.57.0's
`tests/`, vendored verbatim apart from four documented adjustments. It runs
against the binary this repo builds.

That is what makes "drop-in replacement" a checkable claim rather than an
aspiration: ~1,830 tests covering the full V1 language — expressions, the
builtin functions, modules, imports, attributes, submodules, dotenv, shell
settings, error messages, exit codes, shell completions — all executed against
`just-next`'s `just`.

Do not write new tests here. This directory should stay diffable against
upstream so it can be resynced. just-next's own tests live in `tests/next/`.

See [`VENDORING.md`](../../VENDORING.md) for the four adjustments and the
resync procedure.

## Running

```sh
cargo test --test integration
```

19 tests are ignored. Those are upstream's own `#[ignore]` markers in
`parallel.rs` and `signals.rs`; none were added here.
