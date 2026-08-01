# Vendoring upstream `just`

`crates/just-v1/` is a copy of [casey/just](https://github.com/casey/just). It
is the V1 engine: justfiles written in upstream's dialect are parsed and run by
this code, in-process, so they behave exactly as they do under the real `just`.

| | |
|---|---|
| Version | 1.57.0 |
| Commit | `13bf03f642f4cec7799c19f1f8f039e1cb3b095d` |
| License | CC0-1.0 — public domain dedication, see `crates/just-v1/LICENSE` |

`tests/integration/` is upstream's test suite, vendored the same way. It drives
the binary this repo builds, which is how V1 compatibility is verified rather
than assumed.

## What was changed

The goal is that resyncing is a directory copy plus a diff review, so the
vendored code is kept as close to verbatim as possible.

**`crates/just-v1/src/`** — verbatim, except `src/main.rs` is deleted. This
crate is consumed as a library; `src/main.rs` in the repo root is the binary,
and it calls `just_v1::run`.

**`crates/just-v1/Cargo.toml`** — rewritten, but every value the source reads
back through `env!("CARGO_PKG_*")` matches upstream exactly. Those values reach
users: `CARGO_PKG_NAME` and `CARGO_PKG_VERSION` are what `--version`,
`--help` and `just_version()` print, so if they drift, the binary stops
identifying itself as `just`. The package is therefore still named `just`; only
the *lib target* is renamed to `just_v1`, so it can be linked alongside this
repo's own `just` library.

Also set: `autobins = false` and `autotests = false` (the deleted `main.rs` and
the relocated tests), and `[lib] doctest = false`.

**`crates/just-v1/`** support files — `build.rs`, `completions/`, `etc/`,
`CHANGELOG.md`, `LICENSE`, `GRAMMAR.md`, `README.md` and `examples/` are copied
because the source `include_str!`s some of them and the test suite reads others.

**`tests/integration/`** — verbatim except for four path and flag adjustments,
all documented inline:

- `lib.rs` defines `V1_ROOT`, pointing at `crates/just-v1`.
- `changelog.rs`, `examples.rs`, `readme.rs` read their fixtures from `V1_ROOT`
  rather than the repo root. These tests assert on upstream's *repository
  files* — its changelog, its example justfiles, its README's heading style —
  not on `just`'s behaviour, so they must look at the vendored copies.
- `readme.rs` additionally passes `--legacy` when dumping README justfiles. One
  block in upstream's README is a worked example of the per-line shell isolation
  that just-next deliberately changes, and it is character-for-character
  identical to the V2 idiom. See "Ambiguity" in `src/dispatch.rs`.

**Root `Cargo.toml`** — the root package version tracks the upstream release
this is compatible with, because the binary reports it. Crates upstream used as
ordinary dependencies (`dirs`, `num_cpus`, `nix`, `pulldown-cmark`) are
dev-dependencies here, since the vendored tests now live in a different package.

**`Cargo.lock`** — upstream's, adopted wholesale. Dependency drift is not
cosmetic: resolving clap to 4.5 instead of 4.6 changed how one positional
argument parsed and broke the CLI. Keep taking upstream's lockfile on resync.

## Resyncing

```sh
git clone --depth 1 https://github.com/casey/just.git /tmp/just-upstream

cd /tmp/just-upstream && git rev-parse HEAD    # record this in the table above

rsync -a --delete /tmp/just-upstream/src/         crates/just-v1/src/
rsync -a --delete /tmp/just-upstream/completions/ crates/just-v1/completions/
rsync -a --delete /tmp/just-upstream/etc/         crates/just-v1/etc/
rsync -a --delete /tmp/just-upstream/examples/    crates/just-v1/examples/
cp /tmp/just-upstream/{build.rs,CHANGELOG.md,LICENSE,GRAMMAR.md,README.md} crates/just-v1/
cp /tmp/just-upstream/Cargo.lock .
rm crates/just-v1/src/main.rs

# Re-apply the four test edits above, then:
rsync -a --delete /tmp/just-upstream/tests/ tests/integration/
```

Then:

1. Diff upstream's `Cargo.toml` against `crates/just-v1/Cargo.toml` and carry
   over any dependency changes, plus `version` into the root manifest.
2. Check `crates/just-v1/src/setting.rs` against `V1_ONLY_SETTINGS` in
   `src/dispatch.rs`. A new upstream setting that just-next does not know about
   is a new V1 signal, and missing one means those justfiles may route to V2.
3. `cargo test`. The integration suite is the acceptance criterion: it should be
   fully green, with the only ignored tests being upstream's own in `parallel.rs`
   and `signals.rs`.
