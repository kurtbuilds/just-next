# just-next

A modern reimplementation of [just](https://github.com/casey/just), the command runner. `just-next` keeps the core simplicity of `just` while fixing long-standing ergonomic issues and adding automatic environment setup.

# Installation

  cargo install --git https://github.com/kurtbuilds/just-next.git

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

`just-next` maintains full backwards compatibility—it automatically detects legacy justfiles and delegates to the original `just` binary.

## Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `set next` | false | Force next-style parsing (disable legacy detection) |
| `set dotenv` | ".env" | Load `.env` files. Set to false to disable |
| `set export` | true | Export all variables to environment |
| `set positional-arguments` | true | Enable `$1`, `$2`, `$@` in recipes |
| `set venv = "path"` | auto | Path to Python virtualenv |
| `set just = "path"` | search PATH | Path to legacy `just` binary |

## Backwards Compatibility

`just-next` automatically detects legacy justfiles by looking for:

- `:=` assignment syntax
- `env_var()`, `env_var_or_default()` function calls
- `if`/`else` expressions
- Other just-specific syntax

When legacy syntax is detected, `just-next` transparently delegates to the original `just` binary. This means you can use `just-next` as a drop-in replacement without breaking existing justfiles.

To force next-style parsing, add `set next` to your justfile:

```just
set next

build:
    cargo build
```

## Command Line Usage

```
just-next [OPTIONS] [RECIPE] [ARGS...]

Options:
  -n, --dry-run              Print commands without executing
  -q, --quiet                Suppress command echoing
  -l, --list                 List available recipes
  -f, --justfile <PATH>      Use specific justfile
  -d, --working-directory    Set working directory
```

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
