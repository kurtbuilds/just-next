# just-next

A modern reimplementation of [just](https://github.com/casey/just), the command runner. `just-next` keeps the core simplicity of `just` while fixing long-standing ergonomic issues and adding automatic environment setup.

## Why?

`just` is excellent, but after 10 years it has accumulated some warts:

1. **Foreign syntax** - Export statements and variable assignments use `:=` and string concatenation instead of familiar bash syntax
2. **No state between lines** - Each recipe line runs in a separate shell, breaking variable assignments and `shift`
3. **Manual environment setup** - You must manually add `node_modules/.bin`, activate virtualenvs, etc.
4. **Quoting issues** - Variadic arguments don't preserve quoting, requiring shebang workarounds

`just-next` fixes all of these while maintaining backwards compatibility - it automatically detects legacy justfiles and delegates to the original `just` binary.

## Installation

```bash
cargo install --path .
```

## Syntax Comparison

### Export Statements

<table>
<tr><th>Original just</th><th>just-next</th></tr>
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
<tr><th>Original just</th><th>just-next</th></tr>
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
Variables persist across lines automatically.

</td>
</tr>
</table>

### Using `shift` with Variadic Arguments

<table>
<tr><th>Original just</th><th>just-next</th></tr>
<tr>
<td>

```just
run NAME *ARGS:
    #!/bin/bash
    shift
    ./program "$NAME" "$@"
```
Requires shebang because `shift` doesn't work across lines.

</td>
<td>

```just
run NAME *ARGS:
    shift
    ./program "$NAME" "$@"
```
`shift` works naturally, modifying `$@` for subsequent lines.

</td>
</tr>
</table>

### Proper Argument Quoting

<table>
<tr><th>Original just</th><th>just-next</th></tr>
<tr>
<td>

```just
test *ARGS:
    cargo test {{ARGS}}
```
Arguments with spaces are not properly quoted. `ARGS` is just a string.

</td>
<td>

```just
test *ARGS:
    cargo test "$@"
```
Use standard `$@` with proper quoting semantics.

</td>
</tr>
</table>

### Export Within Recipes

<table>
<tr><th>Original just</th><th>just-next</th></tr>
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
set venv = "my-custom-venv"
set dotenv = ".env.production"
```

## Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `set next` | false | Force next-style parsing (disable legacy detection) |
| `set dotenv` | true | Load `.env` files |
| `set dotenv = "path"` | - | Load specific dotenv file |
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
      --next                 Force next-style parsing
      --legacy               Force delegation to original just
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
    shift
    cargo test "$@"

# Deploy to environment (default: staging)
deploy ENV="staging":
    echo "Deploying to $ENV"
    ./scripts/deploy.sh "$ENV"

# Build with captured output
build:
    OUTPUT=$(cargo build --release 2>&1)
    echo "Build complete"
    echo "$OUTPUT" | tail -5
```

## How It Works

Unlike original `just` which runs each line in a separate shell, `just-next` executes recipes line-by-line while tracking state:

1. Each command is printed in bold, then executed
2. Variable assignments (`FOO=bar` or `FOO=$(cmd)`) are captured and stored
3. Exports are added to the environment for subsequent commands
4. `shift` modifies the positional argument list
5. All state carries forward to the next line

For recipes with shebangs (`#!/bin/bash`), the entire recipe is still executed as a single script.

## License

MIT
