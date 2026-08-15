# just-next

A drop-in replacement for [just](https://github.com/casey/just), the command
runner. `just-next` keeps the core simplicity of `just` while fixing
long-standing ergonomic issues and adding automatic environment setup.

It installs a binary called `just`, and it is a real drop-in: upstream `just`
1.57.0 is vendored in and runs existing justfiles in-process, so they behave
exactly as before — same output, same error messages, same exit codes. There is
no second binary to keep on your `PATH`. Upstream's own test suite, all ~1,830
tests of it, runs against this binary in CI.

# Installation

  cargo install --git https://github.com/kurtbuilds/just-next.git

This installs a binary named `just`. If you already have upstream `just`
installed, make sure this one comes first on your `PATH`.

## Why?

`just` radically simplified writing commands for projects in any language, but it is nearly 10 years old and stuck with some early decisions. `just-next` simplifies the syntax of `just`, solves common gotchas, and intelligently handles environment setup.

## Syntax Comparison

### Export Statements

<table>
<tr><th>just</th><th>just-next</th></tr>
<tr>
<td>

```just
export PATH := env_var("PATH") + ":node_modules/.bin"
export FOO := "bar"
```

</td>
<td>

```just
export PATH="node_modules/.bin:$PATH"
export FOO="bar"
```

</td>
</tr>
</table>

### Variable Assignments in Recipes

<table>
<tr><th>just</th><th>just-next</th></tr>
<tr>
<td>

```just
build:
    #!/bin/bash
    FOO=$(echo bar)
    echo $FOO
```
Each line runs in a separate shell, so you need a shebang to preserve state.

</td>
<td>

```just
build:
    FOO=$(echo bar)
    echo $FOO
```

`just-next` has a notion of environment. Each line is its own shell, but that environment concept persists across lines.

</td>
</tr>
</table>

### Quoting variables

<table>
<tr><th>just</th><th>just-next</th></tr>
<tr>
<td>

```just
run NAME *ARGS:
    #!/bin/bash
    shift
    ./program "$NAME" "$@"
```
`just` treats every argument as a single string, so quoted lists simply do not
work. [The issue](https://github.com/casey/just/issues/208) unfortunately has
been open since 2017. With multiple arguments, this recipe, combined with `positional-arguments`, is the only workaround.

</td>
<td>

```just
run NAME *ARGS:
    ./program $NAME $ARGS
```
`just-next` automatically quotes all arguments, and handles argument lists correctly.

</td>
</tr>
</table>

### Export Within Recipes

<table>
<tr><th>just</th><th>just-next</th></tr>
<tr>
<td>

```just
build:
    #!/bin/bash
    export CC=clang
    make
```
Requires shebang to persist exports.

</td>
<td>

```just
build:
    export CC=clang
    make
```
Exports persist to subsequent commands.

</td>
</tr>
</table>

## Running Recipes in Another Folder

Prefix a recipe with a path to run it from another folder's justfile:

```bash
just api/build          # runs `build` from api/justfile
just crates/web/serve   # nested paths work too
just api/               # runs api/justfile's first recipe
just -l api/            # lists api/justfile's recipes
```

The recipe runs with `api/` as its working directory, so relative paths, `.env`
files, and virtualenv detection all resolve against that folder—exactly as if you
had `cd`'d there first. Unlike the normal justfile search, the path is used as
given: `just api/build` looks only in `api/`, never in its parents.

## Automatic Environment Setup

`just-next` automatically configures your environment, similar to [mise](https://mise.jdx.dev/):

| Feature | Behavior |
|---------|----------|
| `.env` files | Loaded automatically (`.env.local` takes precedence) |
| `node_modules/.bin` | Added to `PATH` if present |
| `target/debug` | Added to `PATH` (respects `$CARGO_TARGET_DIR`) |
| Python virtualenv | Activated automatically (`.venv`, `venv`, or `.uv/venv`) |

All of these happen without any configuration. You can customize with settings:

```just
set venv = "path/to/venv/"
set dotenv = ".env.production"
```

## Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `set next` | false | Force next-style parsing (disable detection) |
| `set dotenv` | ".env" | Load `.env` files. Set to false to disable |
| `set export` | true | Export all variables to environment |
| `set positional-arguments` | true | Enable `$1`, `$2`, `$@` in recipes |
| `set venv = "path"` | auto | Path to Python virtualenv |

## Backwards Compatibility

There is one binary and two engines. Upstream `just` 1.57.0 is vendored into
this repo and called in-process, so an existing justfile runs on the real `just`
code — not a reimplementation of it. `just-next` picks the engine per justfile,
by looking at its syntax.

**Legacy justfiles run on upstream `just`.** These constructs select it:

- `:=` assignments
- `{{ ... }}` interpolation
- backtick command evaluation
- attributes (`[private]`, `[group('x')]`, …)
- `import`, `mod`, `unexport`
- exported parameters (`recipe $FOO:`)
- dependencies with arguments (`build: (setup "x")`)
- settings only upstream has (`set dotenv-load`, `set windows-shell`, …)

**Next-style justfiles run on the new engine.** These select it:

- `export NAME="value"` — shell-style, with no `:=`
- a bare `VAR=value` assignment on a recipe body line
- `export NAME=value` inside a recipe body
- `$PARAM` in a body, referring to one of that recipe's parameters

No `set next` marker is needed for either. When a justfile has none of these
markers — `build:` followed by `cargo build` parses identically under both — it
runs on the new engine, so it gets the automatic environment setup. A plain
justfile runs the same either way, and the setup is the whole of the difference;
withholding it would mean a justfile whose `.env` silently never loads, failing
somewhere far downstream with no mention of the environment. Legacy files opt
*out* by carrying one of the constructs above, not in.

Two escape hatches override detection entirely: `--legacy` forces upstream's
engine, `--next` forces the new one.

**`.env` files load on both engines.** Everything else in the automatic setup is
the new engine's alone, but the `.env` is not, because routing is per *file*: one
`{{ }}` in one recipe would otherwise cost every other recipe in the justfile its
environment, and the failure surfaces as an empty variable in a command that has
nothing to do with the construct that caused it. So a legacy justfile gets its
`.env` loaded before upstream runs — the one place `just-next` departs from
upstream's behaviour on the V1 path. A justfile that configures dotenv itself
(`set dotenv-load`, `set dotenv-path`, …) or an invocation that does
(`--no-dotenv`, `--dotenv-path`, …) is left entirely to upstream, as is anything
run under `--legacy`. Existing environment variables are never overwritten.

### Ambiguity

Some justfiles cannot be told apart, because the two dialects genuinely overlap.
The clearest case is a recipe that relies on `just`'s per-line shell isolation:

```just
foo:
  y=bye
  echo $y   # `y` is undefined here under just
```

That is character-for-character the next-style idiom, so it is read as
next-style and `y` *is* defined on the second line. If you have a justfile that
depends on the old behaviour, run it with `--legacy` or give it a `:=`
assignment. This is the one place where detection changes the meaning of a
*line*, and it is inherent to detecting by syntax rather than by a marker.

Routing an ambiguous file to the new engine also gives it the automatic
environment setup, which is a change in its own right: a `.env` sitting beside a
justfile that never loaded one now loads. That is the intended effect, but it
means a recipe can see variables it did not before. `--legacy` opts out.

## Command Line Usage

```
just [OPTIONS] [<FOLDER>/][RECIPE] [ARGS...]

Options:
  -n, --dry-run              Print commands without executing
  -q, --quiet                Suppress command echoing
  -l, --list                 List available recipes
  -f, --justfile <PATH>      Use specific justfile
  -d, --working-directory    Set working directory
      --legacy               Force the upstream `just` engine
      --next                 Force the next-style engine
```

Legacy justfiles get upstream's full command line — `--dump`, `--fmt`,
`--evaluate`, `--json`, `--completions` and the rest all work as they do in
`just`, because they *are* `just`. The flags above are what the next-style
engine currently supports.

Ambiguous justfiles get them too. The engine choice for a file that could go
either way depends on the invocation as well as the syntax: a flag outside the
list above sends it to upstream, so `just --fmt` formats a plain justfile
instead of failing on an unrecognised argument. Only a file that names its
dialect — a legacy construct, `set next`, or an explicit `--legacy`/`--next` —
ignores the command line.

## Development

```sh
cargo test                      # everything
cargo test --test integration   # upstream just's suite, run against this binary
cargo test --test next          # just-next's own features
```

`tests/integration/` is upstream's test suite, vendored. See
[VENDORING.md](VENDORING.md) for how the upstream copy is kept in sync.

## Example

```just
set next

export DATABASE_URL="postgres://localhost/myapp"

# Run database migrations
migrate:
    diesel migration run

# Run tests with optional filter
test *ARGS:
    cargo test $ARGS

# Deploy to environment (default: staging)
deploy ENV=staging:
    echo "Deploying to $ENV"
    ./scripts/deploy.sh $ENV

# Build with captured output
build:
    OUTPUT=$(cargo build --release 2>&1)
    echo "Build complete"
    echo "$OUTPUT" | tail -5
```
