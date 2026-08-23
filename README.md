##

```shell
 ██████╗ ██████╗ ███╗   ██╗████████╗███████╗██╗  ██╗████████╗               
██╔════╝██╔═══██╗████╗  ██║╚══██╔══╝██╔════╝╚██╗██╔╝╚══██╔══╝               
██║     ██║   ██║██╔██╗ ██║   ██║   █████╗   ╚███╔╝    ██║                  
██║     ██║   ██║██║╚██╗██║   ██║   ██╔══╝   ██╔██╗    ██║                  
╚██████╗╚██████╔╝██║ ╚████║   ██║   ███████╗██╔╝ ██╗   ██║                  
 ╚═════╝ ╚═════╝ ╚═╝  ╚═══╝   ╚═╝   ╚══════╝╚═╝  ╚═╝   ╚═╝
```

##

`▐▀` `-` `▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▌`

- **`[Context]`** (`ctx`) is a **`[POSIX]`** shell written in **`[Rust]`** for **`[Performance]`** and **`[Customization]`**, providing a full experience of **`[Safety]`** without memory leaks.

- **`Version`**: **`[v0.70.0]`**

`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`

<details>

<summary>Contents</summary>

## Table of Contents

- [**`[Overview]`**](#overview)
- [**`[Building]`**](#building)
- [**`[Installation]`**](#installation)
  - [**`[Cargo (direct)]`**](#cargo-direct)
  - [**`[Make]`**](#make)
  - [**`[Meson]`**](#meson)
  - [**`[Ninja]`**](#ninja)
  - [**`[CMake]`**](#cmake)
  - [**`[MCX (package manager)]`**](#mcx-package-manager)
- [**`[Testing]`**](#testing)
- [**`[Linting]`**](#linting)
- [**`[Auditing]`**](#auditing)
- [**`[Debugging]`**](#debugging)
- [**`[Profiling]`**](#profiling)
- [**`[Continuous integration]`**](#continuous-integration)
- [**`[Cargo.toml release profile]`**](#cargotoml-release-profile)
- [**`[Usage]`**](#usage)
  - [**`[CLI Flags]`**](#cli-flags)
    - [**`[Info]`**](#info)
    - [**`[Command Execution]`**](#command-execution)
    - [**`[Environment]`**](#environment)
    - [**`[Shell Mode]`**](#shell-mode)
    - [**`[Startup]`**](#startup)
    - [**`[Output and Debug]`**](#output-and-debug)
    - [**`[Terminal]`**](#terminal)
    - [**`[History]`**](#history)
    - [**`[Prompt]`**](#prompt)
    - [**`[Jobs]`**](#jobs)
    - [**`[Signals]`**](#signals)
    - [**`[Security]`**](#security)
    - [**`[Configuration]`**](#configuration)
    - [**`[Config Generation]`**](#config-generation)
    - [**`[Branding]`**](#branding)
- [**`[Locations]`**](#locations)
- [**`[Architecture]`**](#architecture)
  - [**`[Source Layout]`**](#source-layout)
  - [**`[Data Flow Pipeline]`**](#data-flow-pipeline)
- [**`[Syntax]`**](#syntax)
  - [**`[Simple Commands]`**](#simple-commands)
  - [**`[Multi-Line Input]`**](#multi-line-input)
  - [**`[Pipelines]`**](#pipelines)
  - [**`[Logical Operators]`**](#logical-operators)
  - [**`[Redirects]`**](#redirects)
  - [**`[Variable Expansion]`**](#variable-expansion)
  - [**`[Parameter Expansion Extras]`**](#parameter-expansion-extras)
  - [**`[Command Substitution]`**](#command-substitution)
  - [**`[Arithmetic Expansion]`**](#arithmetic-expansion)
  - [**`[Floating Point Math]`**](#floating-point-math)
  - [**`[Tilde Expansion]`**](#tilde-expansion)
  - [**`[Brace Expansion]`**](#brace-expansion)
  - [**`[Glob Expansion]`**](#glob-expansion)
  - [**`[ANSI-C Quoting]`**](#ansi-c-quoting)
  - [**`[Regex Matching]`**](#regex-matching)
  - [**`[Test Expressions]`**](#--test-expressions)
  - [**`[Associative Arrays]`**](#associative-arrays)
  - [**`[Control Flow]`**](#control-flow)
- [**`[Builtins]`**](#builtins)
  - [**`[All Builtins]`**](#all-builtins)
  - [**`[Builtin Docs]`**](#builtin-docs)
  - [**`[Test / "\[" Operators]`**](#test---operators)
- [**`[Autosuggestions]`**](#autosuggestions)
- [**`[History]`**](#history-1)
- [**`[Editor]`**](#editor)
  - [**`[Line Editor]`**](#line-editor)
  - [**`[Emacs Mode (default)]`**](#emacs-mode-default)
  - [**`[Vi Mode]`**](#vi-mode)
  - [**`[Kill Ring \& Undo/Redo]`**](#kill-ring--undoredo)
  - [**`[Syntax Highlighting]`**](#syntax-highlighting)
  - [**`[Custom Widgets]`**](#custom-widgets)
- [**`[Prompt]`**](#prompt-1)
  - [**`[PromptDisplay Structure]`**](#promptdisplay-structure)
  - [**`[Dynamic Variables]`**](#dynamic-variables)
  - [**`[Format Fields]`**](#format-fields)
  - [**`[Right Prompt (RPROMPT)]`**](#right-prompt-rprompt)
  - [**`[Transient Prompt]`**](#transient-prompt)
  - [**`[Instant Prompt]`**](#instant-prompt)
  - [**`[Async Prompt]`**](#async-prompt)
  - [**`[Prompt Appearance]`**](#prompt-appearance)
- [**`[Signals]`**](#signals-1)
- [**`[Variables]`**](#variables)
- [**`[Modules]`**](#modules)
- [**`[Coloring]`**](#coloring)
  - [**`[Auto-Detection]`**](#auto-detection)
  - [**`[Graceful Degradation]`**](#graceful-degradation)
- [**`[Configuration]`**](#configuration-1)
  - [**`[Config Locations]`**](#config-locations)
  - [**`[Schema]`**](#schema)
    - [**`[history]`**](#history-2)
    - [**`[cursor]`**](#cursor)
    - [**`[prompt]`**](#prompt-2)
    - [**`[box_config]`**](#box_config)
    - [**`[colors]`**](#colors)
    - [**`[symbols]`**](#symbols)
    - [**`[ascii]`**](#ascii)
    - [**`[execution]`**](#execution)
    - [**`[startup]`**](#startup-1)
    - [**`[editor]`**](#editor-1)
    - [**`[display]`**](#display)
    - [**`[signals]`**](#signals-2)
    - [**`[environment]`**](#environment-1)
    - [**`[branding]`**](#branding-1)
    - [**`[autosuggest]`**](#autosuggest)
    - [**`[keybindings]`**](#keybindings)
    - [**`[jobs]`**](#jobs-1)
    - [**`[clipboard]`**](#clipboard)
    - [**`[integration]`**](#integration)
    - [**`[security]`**](#security-1)
    - [**`[performance]`**](#performance)
    - [**`[modes]`**](#modes)
    - [**`[dynamic]`**](#dynamic)
    - [**`[python]`**](#python)
  - [**`[Hot-Reload]`**](#hot-reload)
- [**`[Python]`**](#python-1)
  - [**`[Themes]`**](#themes)
  - [**`[Plugins]`**](#plugins)
  - [**`[TUI Mode]`**](#tui-mode)
  - [**`[Virtual Environments]`**](#virtual-environments)
  - [**`[Isolation]`**](#isolation)
- [**`[Jobs]`**](#jobs-2)
- [**`[Trapping]`**](#trapping)

---

</details>

<details>
<summary id="overview">Overview</summary>

## Overview

| Category | What |
|----------|------|
| Lexer | `$'...'` ANSI-C quoting, `[[`/`]]` tokens, `>>\|` clobber, `;&`/`;;&` case fall-through |
| Shell syntax | `if`/`for`/`while`/`case`/functions/subshells/brace groups, `coproc` background coprocesses |
| Builtins | 56 builtins including `coproc`, `fc`, `disown`, `suspend`, `complete`, `compgen`, `mapfile`, `readarray` |
| POSIX compliance | Full POSIX shell semantics: positional params, `$@`/`$*` splitting, `set`/`shopt` options, `exec`, `trap`, `wait -n`, `pipefail` |
| Variable expansion | `${var:-def}`, `${var//old/new}`, `${var:=val}`, `${#var}`, `${var@Q/@E/@U/@u/@a/@P}` |
| Parameter extras | `${var:u}` uppercase, `${var:l}` lowercase, `${var:r}` strip ext, `${var:e}` ext only, `${var:t}` basename, `${var:h}` dirname |
| Arithmetic | `let`, `(( ))`, `math` (float), operator precedence, ternary, comma, hex/octal/binary literals |
| Regex | `[[ $var =~ pattern ]]` with `BASH_REMATCH` array, `=~` operator |
| Associative arrays | `declare -A`, `${name[key]}`, `${(k)name}`, `${(v)name}` |
| Indexed arrays | `declare -a`, `${arr[@]}`, `${#arr[@]}`, `mapfile`, `readarray` |
| Process substitution | `<()` and `>()` for feeding commands as file arguments |
| Coprocess | `coproc NAME { cmd; }` — bidirectional pipe to background command |
| Brace expansion | `echo {a,b,c}`, `echo {1..5}`, `echo {a..z..2}` |
| Glob expansion | `*`, `?`, `[abc]`, `[a-z]`, `[!abc]`, `**` (globstar) |
| Multi-line input | Backslash `\` continuation, unclosed quotes, unclosed `$()` |
| Tab completion | Path completion, command name completion, custom completion via `complete -F/-C` |
| Editor modes | Emacs mode with kill ring, undo/redo. Vi insert/normal/visual mode with motions (`w`/`W`/`b`/`f`/`F`/`t`/`T`), operators (`d`/`c`/`y`), visual selections (`d`/`y`/`u`/`U`), `.` repeat |
| Word splitting | IFS-based splitting respects quoting context |
| `[[ ]]` | Full test expressions with pattern matching, regex, grouping, logical operators, `-nt`/`-ot`/`-ef` |
| Shopt options | `extglob`, `globstar`, `nullglob`, `dotglob`, `failglob`, `nocaseglob`, `nocasematch`, `cmdhist`, `lithist`, `lastpipe` |
| Traps | `trap`, `ERR` trap, `DEBUG` trap, `RETURN` trap, pseudo-signals |
| Autosuggestions | Fish-style grey suggestions from history |
| History | Shared across sessions via `flock`, `sync_history` on each prompt |
| Named directories | `hash -d name=path`, `~name` expands to path |
| Prompt | Multi-line cursor format, right prompt, transient, instant, async, PS1 escapes (`\u`, `\h`, `\w`, `\t`, `\d`, `\!`, `\#`, `\v`, `\V`) |
| Syntax highlighting | Real-time in the line editor |
| Color system | Auto-detect: TrueColor → 256 → 16 → None. Any `#RRGGBB` hex. Wallpaper-based dynamic colors |
| TOML config | 25 sections, 300 fields, hot-reload via `SIGUSR1` |
| Job control | `fg`, `bg`, `jobs`, `disown`, `&`, `wait -n`, process groups |
| Signal traps | `trap 'cmd' SIGINT`, ERR/DEBUG/RETURN traps |
| Modules | `module load/unload/list/info` |
| Bash compat | `BASH_REMATCH`, `LINENO`, `FUNCNAME`, `BASH_SOURCE`, `PIPESTATUS`, `BASH_ALIASES`, `BASH_CMDS`, `COPROC_PID`, `SECONDS` assignment |
| Performance | Zero-copy prompt rendering, allocation-free autosuggestions, single-pass ANSI-aware width calculation |

---

</details>

<details>
<summary id="development">Development</summary>

## Building

| Profile | Command | Flags | Use case |
| ------- | ------- | ----- | -------- |
| Debug | `cargo build` | — | Development iteration, fast compile |
| Release | `cargo build --release` | `opt-level = 3`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true` | Production binary, minimised size |
| Check | `cargo check` | — | Compile-only verification, no artifacts |

```shell
# Compile-only verification (fastest)
cargo check

# Debug build
cargo build

# Release build (optimised)
cargo build --release
```

### Feature flags

| Feature | Default | Enables |
| ------- | ------- | ------- |
| `python` | off | cps Python subsystem: plugins, themes and TUIs through an embedded interpreter |

The default build is fully native and thin — no `pyo3`, no `libpython` linked, zero interpreter startup cost. The `cps` engine is lazy: even in a `python` build the interpreter only initialises when a plugin/theme/TUI is actually configured, and is finalised on exit.

```shell
# Thin default build (no Python)
cargo build --release

# With Python subsystem (libpython dynamically linked)
cargo build --release --features python

# Compile-only verification of the opt-in path
cargo check --features python
```

## Installation

All build systems auto-detect `x86_64`/`aarch64` and select the correct musl target. Cross-compilation files are in `env.mk`, `toolchain.cmake`, and `cross.txt` (generated via `scripts/crossgen.sh`).

### Cargo (direct)

```shell
cargo build --release
# Binary: target/release/ctx
# Install:
install -Dm755 target/release/ctx /bin/ctx
```

### Make

```shell
make build                    # auto-detects arch, builds for host
make install                  # installs to /bin/ctx
make install DESTDIR=/mnt     # staged install
```

### Meson

```shell
./scripts/crossgen.sh                              # generate cross file for host arch
meson setup builddir --cross-file cross.txt
meson compile -C builddir
meson install -C builddir                   # ctx -> /bin/ctx
```

### Ninja

```shell
ninja -f build.ninja                       # build
DESTDIR=/mnt ninja -f build.ninja install  # staged install
```

### CMake

```shell
cmake -B build -DCMAKE_TOOLCHAIN_FILE=toolchain.cmake -DCMAKE_INSTALL_PREFIX=/
cmake --build build
cmake --install build                      # ctx -> /bin/ctx
```

### MCX (package manager)

```shell
mcx -i context
```

**Dependencies:**

| Crate | Version | Purpose |
|-------|---------|---------|
| crossterm | 0.28 | Terminal raw mode, events, colors |
| serde | 1 | Config serialization framework |
| toml | 0.8 | TOML config file parser |
| dirs | 6 | Platform-specific config paths |
| glob | 0.3 | File glob pattern matching |
| libc | 0.2 | POSIX syscall bindings |
| regex | 1 | Regex matching (`regexmatch` builtin) |
| wallust | 3.5 | Wallpaper color extraction (k-means, fast_resize, color spaces) |
| serde_json | 1 | JSON serialization for dynamic color data |
| cps | git | Python subsystem host (plugins/themes/TUI); pyo3 only with `--features python` |

## Testing

```shell
# Run all tests (unit + integration)
cargo test

# Run with stdout/stderr visible
cargo test -- --nocapture

# Run a specific test by name
cargo test -- test_name

# Run with all features and release mode
cargo test --release --all-features
```

## Linting

```shell
# Clippy (lint checks)
cargo clippy -- -D warnings

# Format check
cargo fmt --check

# Format in place
cargo fmt
```

## Auditing

```shell
# Check for security advisories in dependencies
cargo audit
```

## Debugging

```shell
# Build with debug assertions enabled in release
cargo build --profile release-debug  # requires Cargo.toml profile

# Run with RUST_LOG for tracing
RUST_LOG=debug context

# Run with backtrace on panic
RUST_BACKTRACE=1 ctx

# Run under strace for syscall tracing
strace -f -o /tmp/context.strace ./target/release/ctx

# Memory profiling with valgrind
valgrind --tool=massif ./target/release/ctx
ms_print massif.out.* | less
```

## Profiling

```shell
# perf profiling (Linux)
perf record --call-graph dwarf ./target/release/ctx
perf report

# Generate flamegraph
perf script | inferno-collapse-perf > stacks.folded
inferno-flamegraph stacks.folded > flamegraph.svg

# CPU sampling with perf stat
perf stat -e cycles,instructions,cache-misses,faults ./target/release/ctx

# Heap profiling with dhat (requires `dhat` feature)
# Run with DHAT_VALIDATE=1 and parse dhat-heap.json
```

## Continuous integration

```yaml
# Expected CI pipeline (GitHub Actions)
steps:
  - name: Checkout
    run: git checkout ${{ github.ref }}

  - name: Build
    run: cargo build --release

  - name: Test
    run: cargo test --release

  - name: Lint
    run: cargo clippy -- -D warnings

  - name: Format
    run: cargo fmt --check

  - name: Audit
    run: cargo audit
```

## Cargo.toml release profile

```toml
[profile.release]
opt-level = 3         # Optimise for speed
lto = true            # Link-time optimisation
strip = true          # Strip symbols
```

---
</details>

<details>
<summary id="usage">Usage</summary>

## Usage

| Command | Description |
|---------|-------------|
| `context` | Interactive REPL |
| `context -c "cmd"` | Execute command, print output, exit |
| `echo "ls" \| context -s` | Read commands from stdin |
| `context -f script.context` | Execute commands from file |
| `context -e 'echo hello'` | Evaluate code and exit |
| `context -h` / `context --help` | Show help |
| `context -v` / `context --version` | Show version |

### CLI Flags

#### Info

| Short | Long | Description |
|-------|------|-------------|
| `-h` | `--help` | Show help message and exit |
| `-v` | `--version` | Show version number and exit |
| `-V` | `--verbose` | Show detailed version, build info, and arch |
| `-l` | `--license` | Show license information and exit |
| `-a` | `--authors` | Show authors and exit |

#### Command Execution

| Short | Long | Description |
|-------|------|-------------|
| `-c` | `--command CMD` | Execute CMD as a command string, then exit |
| `-s` | `--stdin` | Read commands from standard input, then exit |
| `-f` | `--file FILE` | Read and execute commands from FILE, then exit |
| `-e` | `--eval CODE` | Evaluate CODE as shell code, then exit |

#### Environment

| Short | Long | Description |
|-------|------|-------------|
| `-E` | `--env KEY=VALUE` | Set environment variable KEY to VALUE |
| `-U` | `--unset KEY` | Remove environment variable KEY |
| `-C` | `--chdir DIR` | Change to DIR before executing commands |
| `-W` | `--workdir DIR` | Set working directory to DIR |

#### Shell Mode

| Short | Long | Description |
|-------|------|-------------|
| `-p` | `--posix` | Run in POSIX-conformant mode |
| `-r` | `--restricted` | Run in restricted mode (no cd, no export, etc.) |
| `-i` | `--interactive` | Force interactive mode |
| `-I` | `--no-interactive` | Force non-interactive mode |
| `-b` | `--bash` | Enable bash compatibility shims |

#### Startup

| Short | Long | Description |
|-------|------|-------------|
| `-n` | `--norc` | Don't read the rc file |
| `-N` | `--noprofile` | Don't read profile or startup scripts |
| | `--no-startup` | Skip all startup scripts and run commands |
| | `--no-welcome` | Suppress the welcome message / ASCII art |
| | `--no-integrations` | Skip loading fzf/zoxide integrations |
| | `--no-plugins` | Skip loading Python plugins |
| | `--no-python` | Skip Python plugin subsystem entirely |
| | `--no-dynamic` | Disable dynamic wallpaper colors (wallust) |
| | `--no-ascii` | Suppress ASCII art on startup |
| | `--ascii` | Force ASCII art on startup |
| | `--run-command CMD` | Run CMD after rc loading, before prompt |
| `-H` | `--startup-delay MS` | Delay MS milliseconds before first prompt |

#### Output and Debug

| Short | Long | Description |
|-------|------|-------------|
| `-q` | `--quiet` | Suppress informational output |
| `-Q` | `--verbose` | Enable verbose output |
| `-d` | `--debug` | Enable debug mode with extra diagnostics |
| `-t` | `--trace` | Enable execution tracing |
| `-x` | `--xtrace` | Enable command tracing (like set -x) |
| `-g` | `--log-file FILE` | Write log output to FILE |
| | `--no-log` | Disable logging |
| | `--benchmark` | Run in benchmark mode |

#### Terminal

| Short | Long | Description |
|-------|------|-------------|
| `-M` | `--no-color` | Disable all color output |
| `-F` | `--color` | Force color output even when not a TTY |
| `-R` | `--color-mode MODE` | Set color mode: `true_color`, `256`, `16`, `0` |
| `-u` | `--utf8` | Enable full UTF-8 character support |
| | `--no-utf8` | Disable UTF-8, use ASCII only |
| `-w` | `--width COLS` | Override detected terminal width |
| `-j` | `--height ROWS` | Override detected terminal height |
| `-J` | `--raw` | Enable raw terminal mode |
| | `--no-raw` | Disable raw terminal mode |
| | `--cursor-style STYLE` | Set cursor style: `block`, `beam`, `underline` |
| | `--no-blink` | Disable cursor blinking |
| | `--blink` | Enable cursor blinking |

#### History

| Short | Long | Description |
|-------|------|-------------|
| `-y` | `--no-history` | Disable history recording entirely |
| `-Y` | `--history-file FILE` | Use FILE as the history file |
| `-z` | `--history-size SIZE` | Set max history size to SIZE entries |
| `-Z` | `--no-history-share` | Don't share history across sessions |

#### Prompt

| Short | Long | Description |
|-------|------|-------------|
| | `--no-prompt` | Disable the interactive prompt |
| `-T` | `--prompt-format FMT` | Override prompt format string |
| `-A` | `--prompt-char CHAR` | Override prompt character (default: ❯) |
| | `--no-transient-prompt` | Disable transient prompt rewriting |
| `-L` | `--instant-prompt` | Enable instant prompt from cache |
| | `--no-instant-prompt` | Disable instant prompt |

#### Jobs

| Short | Long | Description |
|-------|------|-------------|
| `-k` | `--no-jobs` | Disable job control |
| `-K` | `--monitor` | Enable job monitoring (like set -m) |

#### Signals

| Short | Long | Description |
|-------|------|-------------|
| | `--no-signals` | Disable signal handling |
| `-S` | `--forward-signals` | Forward signals to child processes |
| | `--no-forward-signals` | Don't forward signals to children |

#### Security

| Short | Long | Description |
|-------|------|-------------|
| `-B` | `--sandbox` | Enable full sandbox mode |
| | `--no-sandbox` | Disable sandbox mode |
| | `--no-exec` | Disable command execution |
| | `--dry-run` | Dry run mode, don't execute commands |

#### Configuration

| Short | Long | Description |
|-------|------|-------------|
| `-o` | `--print-config` | Print current config as JSON and exit |
| `-O` | `--dump-config` | Print current config as TOML and exit |
| `-P` | `--show-config-path` | Print config file path and exit |
| `-D` | `--show-config-dir` | Print config directory path and exit |
| | `--config-file FILE` | Use FILE as the config file |
| | `--no-config` | Don't load any config file |

#### Config Generation

| Short | Long | Description |
|-------|------|-------------|
| `-G` | `--gen-cfg` | Generate default c.toml config and exit |
| `-X` | `--gen-dycfg` | Generate dynamic c.toml (wallust) and exit |

#### Branding

| Short | Long | Description |
|-------|------|-------------|
| | `--app-name NAME` | Override the application name |
| | `--shell-name NAME` | Override the shell name |
| | `--tagline TEXT` | Override the tagline text |

**Startup sequence:**

```
CLI args parsed
     │
     ▼
Config loaded from ~/.config/context/c.toml
     │
     ▼
Environment initialized (SHLVL++, set_defaults applied)
     │
     ▼
Signal handlers installed (sigaction)
     │
     ▼
Executor created (umask applied, env configured)
     │
     ▼
Python engine initialized (PyO3, venv activated, theme/plugins loaded)
     │
     ▼
~/.ctxrc sourced (if exists)
     │
     ▼
Python plugins fire on_startup()
     │
     ▼
[tui_mode] → theme.run() takes terminal, Context exits when done
     │
     ▼
REPL loop begins
```

---
</details>

<details>
<summary id="locations">Locations</summary>

## Locations

| Path | Purpose |
|------|---------|
| `~/.config/context/c.toml` | Main configuration file |
| `~/.context/context.toml` | Fallback config |
| `~/.ctxrc` | Sourced on startup |
| `~/.ctx_history` | Persistent command history |
| `~/.config/context/modules/*/init.context` | Module scripts |
| `~/.config/context/.instant_prompt` | Cached prompt for instant display |
| `~/.config/context/theme.py` | Python theme (user-created) |
| `~/.config/context/plugins/*.py` | Python plugins (user-created) |
| `~/.config/context/t.desc` | Theme/TUI descriptor registry (also `/etc/context/t.desc`, `./t.desc`) |
| `~/.config/context/themes/*.py` | Theme files referenced by `t.desc` |

---
</details>

<details>
<summary id="architecture">Architecture</summary>

## Architecture

### Source Layout

```
src/
├── main.rs                      REPL loop, CLI args (67 flags), signal dispatch, history I/O
│
├── config/
│   ├── schema.rs                25-section Config with serde + Default impls (297 fields)
│   └── loader.rs                TOML load/save, config/history/rc path resolution
│
├── shell/
│   ├── lexer.rs                 Tokenizer: words, operators, quotes, keywords
│   ├── parser.rs                Recursive descent: precedence, associativity
│   ├── ast.rs                   Node enum, RedirKind, CompoundKind
│   ├── executor.rs              Fork/exec, pipes, redirects, job control, globs, aliases
│   ├── builtin.rs               51 builtins, arithmetic parser, shopt, select, getopts
│   ├── env.rs                   Variable store, aliases, traps, readonly, named dirs, assoc arrays
│   ├── expand.rs                Variable expansion, glob matching, tilde, ~user, parameter extras
│   └── signals.rs               Signal handlers, atomics, child/foreground state
│
├── python/
│   ├── mod.rs                   PythonEngine, venv activation, init with panic catching
│   ├── theme.rs                 ThemeEngine: load, render_prompt, render_right_prompt, run (full TUI)
│   └── plugin.rs                PluginManager: auto-discover all public functions as hooks
│
└── terminal/
    ├── color.rs                 Hex→ANSI conversion, ColorCapability detection
    ├── prompt.rs                PromptDisplay, format variables, CWD shortening, transient
    ├── editor.rs                Line editor: char-based cursor, syntax highlight, autosuggest
    ├── art.rs                   ASCII art, box rendering, welcome screen
    └── raw.rs                   Terminal raw mode guard (RAII)
```

### Data Flow Pipeline

```
   User types input and presses Enter
            │
            ▼
   ┌─────────────────┐
   │  editor.rs      │  Line editor with syntax highlighting, autosuggestions
   └────────┬────────┘
            │
            ▼
   ┌─────────────────┐
   │  lexer.rs       │  Tokenize input into Token stream
   └────────┬────────┘
            │
            ▼
   ┌─────────────────┐
   │  parser.rs      │  Recursive descent → AST (Node tree)
   └────────┬────────┘
            │
            ▼
   ┌─────────────────┐
   │  executor.rs    │  Walk AST, fork/exec, manage jobs
   └────────┬────────┘
            │
            ├──→ expand.rs    $var, ${}, $(), ~, brace expansion, named dirs
             ├──→ builtin.rs   Dispatch to 51 built-in handlers
            └──→ exec_external() Fork + execvp with redirects
```

---
</details>

<details>
<summary id="syntax">Syntax</summary>

## Syntax

### Simple Commands

```sh
$ ls -la /tmp
$ echo "hello world"
$ VAR=value command args
$ export PATH="/system/local/bin:$PATH"
```

### Multi-Line Input

Commands can span multiple lines. The shell detects incomplete input and shows a `> ` continuation prompt:

```sh
$ echo "hello \
> world"
hello world

$ echo "line one
> line two"
line one
line two

$ echo $(seq 1 3)
1
2
3
```

Trailing backslash (`\`), unclosed single/double quotes, and unclosed `$(` all trigger continuation. Press **Ctrl-C** during a continuation to cancel the current input and return to the primary prompt.

### Pipelines

```sh
$ cmd1 | cmd2 | cmd3
$ cat file | grep pattern | wc -l
```

Each command runs in its own process group. Shell waits for all children. Returns exit status of the **last** command.

### Logical Operators

| Syntax | Description |
|--------|-------------|
| `cmd1 && cmd2` | Run cmd2 only if cmd1 succeeds |
| `cmd1 \|\| cmd2` | Run cmd2 only if cmd1 fails |
| `cmd1 ; cmd2` | Run both unconditionally |
| `! cmd` | Invert exit status |

### Redirects

| Syntax | Description |
|--------|-------------|
| `cmd > file` | Stdout → file (truncate) |
| `cmd >> file` | Stdout → file (append) |
| `cmd < file` | Stdin ← file |
| `cmd 2> file` | Stderr → file |
| `cmd N> file` | fd N → file |
| `cmd &> file` | Both stdout + stderr → file |
| `cmd &>> file` | Both stdout + stderr → file (append) |
| `cmd << EOF` | Here document |
| `cmd <<< word` | Here string (feeds `word\n` to stdin) |
| `cmd <& N` | Duplicate fd N to stdin |
| `cmd >& N` | Redirect stdout to fd N (e.g., `echo >&2`) |

### Variable Expansion

| Syntax | Description |
|--------|-------------|
| `$var` | Value of var, or empty |
| `${var}` | Same |
| `${var:-default}` | Use `default` if var unset or empty |
| `${var:+alt}` | Use `alt` if var is set and non-empty |
| `${var:=default}` | Assign `default` to var if unset/empty, then expand |
| `${var=value}` | Assign `value` to var, then expand |
| `${var?error}` | Print error if var unset |
| `${var#pattern}` | Remove shortest prefix matching glob pattern |
| `${var##pattern}` | Remove longest prefix matching glob pattern |
| `${var%pattern}` | Remove shortest suffix matching glob pattern |
| `${var%%pattern}` | Remove longest suffix matching glob pattern |
| `${var/old/new}` | Replace first occurrence |
| `${var//old/new}` | Replace all occurrences |
| `${#var}` | Length of var |
| `${##var}` | Number of characters in var |

### Parameter Expansion Extras

| Syntax | Description |
|--------|-------------|
| `${var:u}` | Convert to uppercase |
| `${var:l}` | Convert to lowercase |
| `${var:r}` | Remove file extension |
| `${var:e}` | Extract file extension |
| `${var:t}` | Tail (basename) |
| `${var:h}` | Head (dirname) |

```sh
$ x="hello-world.txt"
${x:u}     → "HELLO-WORLD.TXT"
${x:r}     → "hello-world"
${x:e}     → "txt"
${x:t}     → "hello-world.txt"  (same as basename)
${x:h}     → "."                (same as dirname)
```

### Command Substitution

```sh
$ echo $(date)
$ echo `date`
```

### Arithmetic Expansion

```sh
$ let x = 2 + 3 * 4       # 14
$ let y = (10 - 3) * 2    # 14
$ let z = -5 + 3           # -2
$ (( x + y ))             # 28
```

### Floating Point Math

```sh
$ math 2.5 * 3.14          # 7.85
$ math 10 / 3              # 3.333333
$ math (2 + 3) * 4.0       # 20
$ math -5.5 + 3.2          # -2.3
```

Operators: `+`, `-`, `*`, `/`, `%`, parentheses. Supports scientific notation. Precision controlled by `[execution] float_precision` (default: 6).

### Tilde Expansion

| Syntax | Expands to |
|--------|-----------|
| `~` | `$HOME` |
| `~user` | User's home (from `/etc/passwd`) |
| `~/file` | `$HOME/file` |

### Brace Expansion

```sh
$ echo {a,b,c}             → a b c
$ echo file{1..3}          → file1 file2 file3
$ echo {a,b}{1,2}          → a1 a2 b1 b2
```

### Glob Expansion

| Pattern | Description |
|---------|-------------|
| `*` | Match zero or more characters |
| `?` | Match exactly one character |
| `[abc]` | Match any character in set |
| `[a-z]` | Match any character in range |
| `[!abc]` / `[^abc]` | Match any character NOT in set |

No matches → original pattern kept as-is.

### ANSI-C Quoting

```sh
$ echo $'hello\nworld'        # newline
$ echo $'\t탭\t'               # tab
$ echo $'\x41\x42\x43'        # hex: ABC
$ echo $'\101\102\103'        # octal: ABC
$ echo $'\u0041\u0042'        # unicode: AB
```

Supported escapes: `\n`, `\t`, `\r`, `\\`, `\'`, `\"`, `\a`, `\b`, `\e`, `\f`, `\v`, `\0NNN` (octal), `\xHH` (hex), `\uHHHH` (unicode).

### Regex Matching

```sh
$ regexmatch "hello world" "hello (\\w+)" CAPTURED
$ echo $MATCH              → "hello world"
$ echo $MATCH_0            → "hello"
$ echo $MATCH_1            → "world"
$ echo $CAPTURED           → "world"
```

Uses the `regex` crate. Sets `$MATCH`, `$MATCH_0`, `$MATCH_1`, etc.

### [[ ]] Test Expressions

```sh
[[ -f file.txt ]]          # file test
[[ -d /tmp ]]              # directory test
[[ $var == "hello" ]]      # string equality
[[ $var =~ ^[0-9]+$ ]]    # regex match
[[ $var == *.txt ]]        # glob matching
[[ -n $var && -f $file ]]  # logical AND
[[ $a -eq 1 || $b -gt 2 ]] # logical OR
[[ ! -z $var ]]            # negation
[[ ( $a == 1 ) && $b ]]    # grouping
```

All test operators: `-f`, `-d`, `-e`, `-r`, `-w`, `-x`, `-s`, `-L`, `-S`, `-p`, `-c`, `-n`, `-z`. Binary: `==`, `!=`, `=~`, `-eq`, `-ne`, `-lt`, `-le`, `-gt`, `-ge`. Operators: `&&`, `||`, `!`, `()` grouping.

### Associative Arrays

```sh
$ declare -A colors
$ colors[red]="#ff0000"
$ colors[green]="#00ff00"
$ echo ${colors[red]}       → #ff0000
$ echo ${(k)colors}         → red green  (keys)
$ echo ${(v)colors}         → #ff0000 #00ff00  (values)
$ echo ${#colors}           → 2  (length)
$ unset colors[red]
```

### Control Flow

**if/elif/else:**

```sh
if cmd; then
  echo true
elif other; then
  echo maybe
else
  echo false
fi
```

**for:**

```sh
for i in 1 2 3; do echo $i; done
for arg; do echo $arg; done    # iterates over "$@"
```

**while / until:**

```sh
while read line; do echo "$line"; done < file.txt
until ping -c1 host; do sleep 1; done
```

**case:**

```sh
case "$1" in
  start)  echo starting ;;
  stop)   echo stopping ;;
  *)      echo unknown ;;
esac
```

**Functions:**

```sh
greet() { echo "Hello, $1"; }
greet world

# Local variables
count() {
    local i=0
    for arg in "$@"; do
        local i=$((i + 1))
    done
    echo "$i args"
}
count a b c    # → "3 args"
```

Functions support local variables via `local`. Scope is managed with a scope stack — local variables shadow outer ones and are removed on return. Positional parameters `$1`, `$@`, `$#` are set per call.

**Subshells / Groups:**

```sh
(cd /tmp && ls)       # subshell (forked)
{ echo a; echo b; }   # brace group (no fork)
```

**Process Substitution:**

```sh
diff <(ls /dir1) <(ls /dir2)     # compare directory listings
while read line; do ... done < <(grep pattern file)  # feed command output
```

`<()` runs command and provides its stdout as a readable file descriptor. `>()` provides writable stdin.

**Coprocess:**

```sh
coproc mycoproc { cat; }          # start coprocess
echo "hello" >&${mycoproc[1]}     # write to coprocess stdin
read -t 1 line <&${mycoproc[0]}   # read from coprocess stdout
```

Coprocesses run in the background with a bidirectional pipe. Access via `COPROC_PID`, `COPROC`, and named `COPROC_<name>_PID`.

**Case fall-through:**

```sh
case "$1" in
  start)  echo starting ;;&     # fall through to next pattern
  run)    echo running ;;
  stop)   echo stopping ;;&     # fall through to all remaining patterns
  *)      echo other ;;
esac
```

`;;&` checks the next pattern. `;;&` checks all remaining patterns.

---
</details>

<details>
<summary id="builtins">Builtins</summary>

## Builtins

### All Builtins

| Command | Description |
|---------|-------------|
| `cd [dir]` | Change directory. `cd -` → `$OLDPWD`. `cd` alone → `$HOME`. |
| `exit [N]` | Exit with status N (default: last status) |
| `export NAME=VAL` | Set and export env var. `export` → print all exports. |
| `unset NAME...` | Remove variables (fails if readonly) |
| `alias NAME=VAL` | Define alias. `alias` → print all. |
| `unalias NAME` | Remove alias |
| `source FILE` / `. FILE` | Execute FILE as shell commands |
| `history [N]` | Show last N entries. `-c` clears. `-d` delete entry. |
| `set [NAME=VAL]` | Set shell variable. `-e`, `-u`, `-x`, `-a`, `-b`, `-B`, `-h`, `-o` options. |
| `env` | Print all variables as NAME=VALUE |
| `pwd` | Print working directory |
| `type NAME` | Check if command is builtin, function, or external |
| `which NAME` | Print path to command |
| `echo [-neE] [args]` | Print args. `-n` no newline. `-e` process escapes. `-E` disable escapes. |
| `printf FMT [args]` | POSIX printf with `%b`, `%q`, `%T` extensions |
| `test EXPR` / `[ EXPR ]` | Conditional test with full operator set |
| `let NAME=EXPR` | Arithmetic evaluation |
| `(( EXPR ))` | Arithmetic evaluation (syntactic sugar) |
| `math EXPR` | Floating point arithmetic |
| `exec CMD [args]` | Replace shell with CMD. `-l` login shell. `-a NAME` set argv[0]. |
| `trap 'CMD' SIG` | Set signal handler. ERR/DEBUG/RETURN traps supported. |
| `pushd [dir]` | Push current dir, cd to dir. Supports `+n`/`-n` rotation. |
| `popd` | Pop and cd to top. Supports `+n`/`-n` removal. |
| `dirs [-v] [-l] [-p]` | Print directory stack. `-v` numbered, `-l` long paths, `-p` one-per-line. |
| `readonly NAME` | Mark variable as read-only |
| `declare [-x] [-r] [-A] [-a] [-i] [-l] [-u] [-n] [-g] [-t] NAME=VAL` | Declare variable with attributes |
| `typeset` | Alias for `declare`. `-f` prints function body. |
| `hash -d name=path` | Define named directory. `hash -r` clear. `hash -p` add to lookup table. |
| `builtin CMD` | Verify CMD is a builtin |
| `shopt [-s] [-u] [opt]` | Set/unset shell options (extglob, globstar, nullglob, etc.) |
| `jobs` | List background jobs. `-l` long. `-p` PIDs only. `-r` running. `-s` stopped. |
| `fg [N]` | Bring job N to foreground |
| `bg [N]` | Resume job N in background |
| `wait [PID]` | Wait for child process. `-n` wait for any. |
| `kill [-SIG] PID` | Send signal. `%N` resolves job IDs. `-l` list signals. |
| `umask [MASK]` | Get/set file mode creation mask |
| `command [-p] CMD` | Execute CMD ignoring aliases. `-v` prints path. `-V` verbose. |
| `eval STRING` | Evaluate STRING as shell commands |
| `select NAME [in ITEMS]` | Interactive menu selection |
| `getopts OPTSTRING NAME [ARGS]` | Parse positional options |
| `read [-r] [-p PROMPT] [-s] [-a NAME] [-d DELIM] [-t TIMEOUT] [-e] [-i TEXT]` | Read line from stdin. `-e` line editor. `-i` initial text. `-t 0` non-blocking poll. |
| `local NAME=VAL` | Declare local variable in function scope |
| `break [N]` | Break out of loop N levels deep |
| `continue [N]` | Continue to next iteration of loop N levels deep |
| `return [N]` | Return from function with status N |
| `suspend` | Suspend the shell (send SIGTSTP to self) |
| `fc [-s] [first] [last]` | Fix command — list or re-execute history entries |
| `disown [PID]` | Remove job from job table |
| `coproc [NAME] CMD` | Start CMD as a coprocess with bidirectional pipe |
| `enable [-f FILE] [-n NAME]` | Enable/disable builtins. `-f` load from shared library (stub). |
| `complete [-F FUNC] [-C CMD] NAME` | Register completion handler for command. `-p` print. `-r` remove. |
| `compgen [-A ACTION] [WORD]` | Generate completions. `-c` commands. `-f` files. `-d` directories. |
| `mapfile [-d DELIM] [-n COUNT] [-O ORIGIN] [-s COUNT] [-t] [-u FD] [-C CALLBACK] [-c QUANTUM] ARRAY` | Read lines into indexed array |
| `readarray` | Alias for `mapfile` |
| `true` | Return 0 |
| `false` | Return 1 |

### Builtin Docs

**cd:**

| Usage | Description |
|-------|-------------|
| `cd` | `cd $HOME` |
| `cd ~` | `cd $HOME` |
| `cd -` | `cd $OLDPWD` (print new dir) |
| `cd /path` | `cd /path` |

Sets `PWD` and `OLDPWD`. Only leading `~` or `~/` expanded.

**pushd / popd / dirs:**

| Usage | Description |
|-------|-------------|
| `pushd dir` | Push cwd, cd to dir |
| `pushd +2` | Rotate stack so position 2 is on top |
| `pushd` | Swap top two directories |
| `popd` | Remove top, cd there |
| `popd +1` | Remove position 1 without changing cwd |
| `dirs` | Print stack |
| `dirs -v` | Numbered listing |
| `dirs -l` | Long paths (no `~` shortening) |

**trap:**

| Usage | Description |
|-------|-------------|
| `trap 'echo SIGINT received' SIGINT` | Set handler |
| `trap '' SIGINT` | Ignore signal |
| `trap - SIGINT` | Reset to default |
| `trap` | Print all active traps |

**shopt:**

| Usage | Description |
|-------|-------------|
| `shopt` | List all options |
| `shopt -s dotglob` | Enable option |
| `shopt -u dotglob` | Disable option |
| `shopt dotglob` | Query option (exit 0 if on, 1 if off) |

**select:**

```sh
select choice in "Option 1" "Option 2" "Option 3"; do
    echo "You chose: $choice"
    break
done
```

**getopts:**

```sh
while getopts "a:b:cv" opt; do
    case $opt in
        a) echo "arg: $OPTARG" ;;
        c) echo "flag" ;;
    esac
done
```

**kill:**

| Usage | Description |
|-------|-------------|
| `kill 1234` | Send SIGTERM |
| `kill -9 1234` | Send SIGKILL |
| `kill -HUP 1234` | Send SIGHUP |
| `kill -s TERM 1234` | Send SIGTERM by name |

**exec:**

| Usage | Description |
|-------|-------------|
| `exec cmd args` | Replace shell process with cmd (fork+execvp, then exit) |
| `exec 2>file` | Redirect stderr for the shell (no args mode) |
| `exec >file` | Redirect stdout for the shell |

With arguments: applies redirects, forks child that execvp's the command, parent waits then signals shell to exit (true process replacement).
Without arguments: applies redirects to the current shell's file descriptors.

**read:**

| Usage | Description |
|-------|-------------|
| `read var` | Read line from stdin into `var` |
| `read -r var` | Raw read (no backslash interpretation) |
| `read -p "prompt: " var` | Display prompt |
| `read -s var` | Silent mode (no echo) |
| `read -a arr` | Split into array `arr` |
| `read -d ',' var` | Use custom delimiter |
| `read -t 5 var` | Timeout after 5 seconds |

**local:**

| Usage | Description |
|-------|-------------|
| `local var=val` | Declare local variable (function scope only) |

Local variables are visible only within the enclosing function. They shadow outer variables of the same name. Removed when the function returns.

### Test / "[" Operators

**Unary (2 args):**

| Op | True if |
|----|---------|
| `-z` | string is empty |
| `-n` | string is non-empty |
| `-e` | path exists |
| `-f` | is regular file |
| `-d` | is directory |
| `-r` | is readable |
| `-w` | is writable |
| `-x` | is executable |
| `-s` | file has size > 0 |
| `-L` / `-h` | is symlink |
| `-S` | is socket |
| `-p` | is FIFO |
| `-c` | is character device |
| `!` | arg is empty |

**Binary (3 args):**

| Op | True if |
|----|---------|
| `=` / `==` | string equal |
| `!=` | string not equal |
| `-eq` | integer equal |
| `-ne` | integer not equal |
| `-lt` | less than |
| `-le` | less or equal |
| `-gt` | greater than |
| `-ge` | greater or equal |
| `-nt` | file newer than |
| `-ot` | file older than |

### Shell Options (shopt)

| Option | Default | Description |
|--------|---------|-------------|
| `extglob` | off | Enable extended glob patterns: `+( )`, `*( )`, `?( )`, `@( )`, `!( )` |
| `globstar` | off | `**` matches any files and zero or more directories and subdirectories |
| `nullglob` | off | Glob patterns expand to nothing when no matches instead of the literal pattern |
| `dotglob` | off | Include filenames beginning with a dot in glob results |
| `failglob` | off | Throw an error when a glob pattern has no matches |
| `nocaseglob` | off | Globbing is case-insensitive |
| `nocasematch` | off | Pattern matching in `[[ ]]` and `case` is case-insensitive |
| `cmdhist` | off | Save multi-line commands in history as single entries |
| `lithist` | off | Save multi-line commands with newlines instead of semicolons |
| `lastpipe` | off | Run the last command of a pipeline in the current shell process |
| `xpg_echo` | off | `echo` interprets escape sequences by default |

---
</details>

<details>
<summary id="autosuggestions">Autosuggestions</summary>

## Autosuggestions

Fish-style grey suggestions displayed as the user types. Sourced from command history.

| Key | Action |
|-----|--------|
| Right Arrow | Accept full suggestion |
| Alt+Right | Accept one word from suggestion |

| Config Field | Default | Description |
|-------------|---------|-------------|
| `[autosuggest] enabled` | `true` | Enable/disable |
| `[autosuggest] min_chars` | `1` | Min input chars before suggesting |
| `[autosuggest] strategy` | `"history"` | Source strategy |
| `[autosuggest] highlight_color` | `"#6b7280"` | Suggestion text color |

---
</details>

<details>
<summary id="history">History</summary>

## History

| Aspect | Detail |
|--------|--------|
| Storage | In-memory during session, append-only to `~/.ctx_history` via `flock` |
| Sync | `sync_history()` on each prompt cycle; reads new lines from file |
| Deduplication | Consecutive identical commands stored once |
| Max entries | `[history] max_size` (default: 256) |
| Clear | `history -c` → clears in-memory + file |
| Print | `history` → print all, `history 10` → print last 10 |
| Cross-session | `[history] share_across_sessions` (default: true) — `flock`-based append |

| Config Field | Default | Description |
|-------------|---------|-------------|
| `[history] max_size` | `256` | Max entries |
| `[history] file` | `"~/.ctx_history"` | History file path |
| `[history] deduplicate` | `true` | Collapse consecutive duplicates |
| `[history] ignore_space` | `true` | Ignore commands starting with space |
| `[history] share_across_sessions` | `true` | Persist across sessions |
| `[history] sync_on_command` | `true` | Sync from file on each prompt |

---
</details>

<details>
<summary id="editor">Editor</summary>

## Editor

### Line Editor

Raw mode via crossterm. All cursor operations use character-based indexing (not byte-based). Multi-byte UTF-8 safe.

On `SIGWINCH` (terminal resize): entire prompt + input is cleared, re-rendered from scratch, cursor repositioned.

Max input length: `[editor] max_line_length` (default: 4096).

### Emacs Mode (default)

| Key | Action |
|-----|--------|
| Enter | Submit line |
| Ctrl+C | Cancel input (clear line, print `^C`) |
| Ctrl+D | Exit (on empty line) |
| Ctrl+L | Clear screen, redraw prompt |
| Ctrl+A | Move to start of line |
| Ctrl+E | Move to end of line |
| Ctrl+K | Kill from cursor to end of line (kill ring) |
| Ctrl+U | Kill from start of line to cursor (kill ring) |
| Ctrl+W | Delete word backward (kill ring) |
| Ctrl+Y | Yank (paste) from kill ring |
| Ctrl+Z | Undo |
| Ctrl+_ | Redo |
| Ctrl+R | Reverse incremental history search |
| Tab | Tab completion (paths, commands, custom completions) |
| Up | Previous history entry |
| Down | Next history entry |
| Left | Move cursor left |
| Right | Accept autosuggestion (when at end of input) |
| Alt+Right | Accept one word from autosuggestion |
| Alt+F | Accept one word from suggestion / move word forward |
| Home | Move to start of line |
| End | Move to end of line |
| Backspace | Delete character before cursor |
| Delete | Delete character at cursor |

### Vi Mode

Set `[editor] mode = "vi"` to enable.

**Vi Insert Mode** — Same as Emacs mode (Ctrl+C clears, Ctrl+D exits, Ctrl+R searches, Tab completes)

**Vi Normal Mode** — Press `Esc` from insert mode:

| Key | Action |
|-----|--------|
| `h` / `l` | Move left / right |
| `0` / `$` | Move to start / end of line |
| `w` / `W` | Move to next word / WORD |
| `b` / `B` | Move to previous word / WORD |
| `f{char}` / `F{char}` | Find char forward / backward on line |
| `t{char}` / `T{char}` | Till char forward / backward (before char) |
| `i` / `a` | Insert before / after cursor |
| `I` / `A` | Insert at start / end of line |
| `o` / `O` | Open line below / above |
| `x` | Delete character at cursor |
| `D` | Kill to end of line |
| `C` | Change to end of line |
| `dd` | Kill entire line |
| `cc` | Clear line (change entire line) |
| `yy` | Yank entire line |
| `p` / `P` | Paste after / before cursor |
| `u` | Undo |
| `r` | Redo (Ctrl+R) |
| `v` | Enter visual mode |
| `.` | Repeat last change |
| `J` | Join current line with next |
| `~` | Toggle case of character under cursor |
| `gg` / `G` | Go to start / end of input |
| `/pattern` | Search history forward for pattern |
| `?pattern` | Search history backward for pattern |
| `n` / `N` | Repeat search same / opposite direction |
| `Esc` | Return to insert mode (move cursor left 1) |

**Vi Visual Mode** — Press `v` from normal mode:

| Key | Action |
|-----|--------|
| `d` | Delete selection |
| `y` | Yank (copy) selection |
| `u` | Lowercase selection |
| `U` | Uppercase selection |
| `v` | Return to normal mode |

### Tab Completion

Pressing Tab triggers context-aware completion:

- **Command position** (start of line or after pipe/semicolon): completes command names (builtins + PATH executables) and file paths simultaneously
- **Argument position**: completes file paths (directories get trailing `/`)
- **Custom completions**: `complete -F func cmd` registers a function; `complete -C /path/cmd cmd` runs an external command

Single match: inserted directly. Multiple matches: longest common prefix inserted, bell rings. No matches: bell rings.

### Kill Ring & Undo/Redo

- **Kill ring**: `Ctrl+K`, `Ctrl+U`, `Ctrl+W` in Emacs mode save killed text. `Ctrl+Y` yanks the most recent kill.
- **Undo/Redo**: `Ctrl+Z` undoes the last edit. `Ctrl+_` redoes. Both work in Emacs and Vi modes.
- **Vi kill ring**: `x`, `D`, `dd` in Vi normal mode also save to the kill ring.

### Syntax Highlighting

All colors are configurable via `[colors]` hex values. When `syntax_use_dynamic_colors = true`, they are derived from the wallpaper palette instead.

| Element | Config Field | Default | What it highlights |
|---------|-------------|---------|-------------------|
| Commands | `syntax_command` | `#3b82f6` | First word after pipe, semicolon, ampersand, or start of line |
| Variables | `syntax_variable` | `#22d3ee` | `$var`, `${var}`, `$(cmd)` |
| Strings | `syntax_string` | `#f59e0b` | `"double-quoted"` and `'single-quoted'` text |
| Operators | `syntax_operator` | `#22c55e` | `\|`, `&`, `;`, `>`, `<`, `>>`, `&&`, `\|\|`, `=`, `==`, `!=`, `+=`, `-=`, `\x` escapes |
| Flags | `syntax_flag` | `#c084fc` | `--long-flag`, `-f`, `-abc` (any word starting with `-`) |
| Paths | `syntax_path` | `#fb923c` | `/abs/path`, `./relative`, `../parent`, `~/home`, any word containing `/` |
| Numbers | `syntax_number` | `#22d3ee` | `123`, `3.14`, `0xff`, `0o77`, `0b1010` |
| Comments | `syntax_comment` | `#6b7280` | `# ...` to end of line |

### Custom Widgets

All widgets are dispatchable via `[keybindings]`. Built-in keybindings are overridable.

| Widget | Description |
|--------|-------------|
| `HistorySearch` | Reverse incremental history search (Ctrl+R) |
| `HistorySearchUp` | Previous history entry (Up arrow) |
| `HistorySearchDown` | Next history entry (Down arrow) |
| `AcceptSuggestion` | Accept full autosuggestion (Right arrow at end of line) |
| `AcceptSuggestionWord` | Accept one word from autosuggestion (Alt+Right) |
| `MoveToLineStart` | Move cursor to start of line (Ctrl+A) |
| `MoveToLineEnd` | Move cursor to end of line (Ctrl+E) |
| `MoveWordLeft` | Move cursor one word left (Alt+Left) |
| `MoveWordRight` | Move cursor one word right (Alt+Right) |
| `DeleteWordLeft` | Delete word before cursor (Ctrl+W) |
| `DeleteWordRight` | Delete word after cursor (Alt+D) |
| `ClearScreen` | Clear screen and redraw prompt (Ctrl+L) |
| `Redraw` | Redraw prompt without clearing screen |
| `Undo` | Undo last edit (Ctrl+Z) |
| `Redo` | Redo last undo (Ctrl+_) |
| `Yank` | Paste from kill ring (Ctrl+Y) |
| `KillLine` | Kill from cursor to end of line (Ctrl+K) |
| `TransposeChars` | Swap characters before cursor (Ctrl+T) |

Green for success (exit 0), red for error. The command is loaded into the input buffer for re-execution or editing.

---
</details>

<details>
<summary id="prompt">Prompt</summary>

## Prompt

### PromptDisplay Structure

```rust
PromptDisplay {
    lines_above:              Vec<String>,
    input_prefix:             String,
    lines_below:              Vec<String>,
    right_prompt:             String,
    right_prompt_color:       String,
    right_prompt_hide_threshold: f64,
}
```

### Dynamic Variables

All prompt format strings support these variables:

| Variable | Value |
|----------|-------|
| `{cwd}` | Working directory (shortened, `~` for home) |
| `{short_cwd}` | Last 2 path components |
| `{user}` | Username |
| `{host}` | Hostname |
| `{exit_code}` | Last command exit status |
| `{pid}` | Shell PID |
| `{time}` | `HH:MM` |
| `{time_full}` | `HH:MM:SS` |
| `{date}` | `YYYY-MM-DD` |
| `{git}` | Current git branch |
| `{git_dirty_char}` | Dirty indicator (`*` if dirty, else empty) |
| `{git_clean_char}` | Clean indicator |
| `{git_staged_char}` | Staged indicator (`+` if staged, else empty) |
| `{git_untracked_char}` | Untracked indicator (`?` if untracked, else empty) |
| `{git_ahead_char}` | Ahead indicator (`↑` if ahead, else empty) |
| `{git_behind_char}` | Behind indicator (`↓` if behind, else empty) |
| `{git_branch_char}` | Branch symbol (nerd: ``, unicode: `⌿`) |
| `{jobs}` | Number of background jobs |
| `{status}` | `"ok"` or `"error"` |
| `{status_char}` | Current status symbol (✓/⚠/✘) based on last exit code |
| `{duration}` | Command duration (e.g. `1.2s`, `45ms`) |
| `{success_char}` | Success symbol (default `✓`) |
| `{warning_char}` | Warning symbol (default `⚠`) |
| `{error_char}` | Error symbol (default `✘`) |
| `{arrow_char}` | Arrow symbol (default `→`) |
| `{separator_char}` | Separator symbol (default `·`) |
| `{exit_prefix}` | Exit prefix (default `exit`) |
| `{exit_label}` | Exit label (default `exit`) |
| `{exit_code_color}` | Exit code color from config |
| `{note_char}` | Note symbol (default `▸`) |
| `{continuation_char}` | Continuation symbol (default `·`) |
| `{job_char}` | Job indicator (default `&`) |
| `{prompt_char}` | Prompt character from symbols config |
| `{python_venv}` | Python virtualenv name |
| `{node_version}` | Node.js version |
| `{rust_version}` | Rust version |
| `{accent}` | Accent color from config |
| `{bg_err}` | Error background color |
| `{bg_info}` | Info background color |
| `{bg_primary}` | Primary background color |
| `{bg_success}` | Success background color |
| `{bg_warning}` | Warning background color |
| `{version}` | Shell version string |
| `{app_name}` | Application name from branding config |
| `{tagline}` | Tagline from branding config |
| `{author}` | Author from branding config |
| `{config_path}` | Path to c.toml config file |
| `{shell_name}` | Shell name from branding config |
| `{terminal_width}` | Current terminal width in columns |
| `{command}` | Command text (for job notification formats) |
| `{signal}` | Signal number (for stopped notification format) |

### Format Fields (Multi-Line Support)

All format config fields accept either a single string or an array of strings (multi-line).
Empty string = don't display that element. Any UTF-8 character is accepted.

```toml
# Single line
prompt_prefix = "❯ "

# Multi-line (each string = one line)
prompt_prefix = [
    "╭─ {user}@{host}",
    "│  {cwd}",
    "╰─ ❯ "
]
```

This applies to all format fields:

| Config | Field |
|--------|-------|
| `[prompt]` | `top_line_left`, `top_line_right`, `prompt_prefix`, `prompt_suffix`, `right_prompt`, `transient_prompt_format`, `vi_prompt_insert`, `vi_prompt_normal`, `vi_prompt_visual`, `vi_cmd_prompt` |
| `[cursor]` | `format` |
| `[box_config]` | `title` |
| `[display]` | `status_line_format`, `title_bar_format`, `session_info_format` |
| `[startup]` | `welcome_message` |
| `[jobs]` | `done_format`, `stopped_format` |

```toml
# Example: multi-line welcome message
[startup]
welcome_message = [
    "  ╔═════════════════════════╗",
    "  ║  Welcome to {app_name}  ║",
    "  ║  {version}              ║",
    "  ╚═════════════════════════╝"
]
```

### Right Prompt (RPROMPT)

Displayed on the right side of the terminal, hidden when input is too long.

```toml
[prompt]
right_prompt = "{git} {time_full}"
right_prompt_color = "#6b7280"
right_prompt_hide_threshold = 0.8    # hide when input > 80% of terminal width
```

### Transient Prompt

After command execution, the prompt is replaced with a minimal format.

```toml
[prompt]
transient_prompt = true
transient_prompt_format = "{user}@{host} {cwd} ❯ "
```

### Instant Prompt

On startup, displays the last cached prompt immediately while the shell initializes.

```toml
[prompt]
instant_prompt = true
```

### Async Prompt

Prompt is cached for `async_prompt_cache_ms` milliseconds, avoiding re-computation on every keystroke.

```toml
[prompt]
async_prompt = true
async_prompt_cache_ms = 1000
```

### Prompt Appearance

```toml
[prompt]
prompt_bold = true              # bold prompt char
gap_before_prompt = 1           # blank lines before prompt
prompt_color_success = "#22c55e" # prompt char color on success
error_prompt_color = "#ef4444"   # prompt char color on error
newline_before_prompt = false    # blank line before prompt
```

---
</details>

<details>
<summary id="signals">Signals</summary>

## Signals

Uses `sigaction()`.

| Signal | Behavior |
|--------|----------|
| `SIGINT` | Forwards to running child; resets input line if idle |
| `SIGTERM`, `SIGHUP` | Clean exit (history saved) |
| `SIGWINCH` | Redraws editor on terminal resize |
| `SIGUSR1` | Hot-reloads config file |
| `SIGCHLD` | Reaps child processes, updates last exit status |
| `SIGTSTP` | Forwards to running child |
| `SIGPIPE` | Ignored |
| `SIGQUIT`, `SIGTTIN`, `SIGTTOU` | Ignored in shell, default in children |

**Atomic flags** (checked each iteration, no complex logic in handlers):

| Flag | Set by |
|------|--------|
| `SHOULD_EXIT` | `SIGTERM`, `SIGHUP` |
| `TRAP_SIGNAL` | `SIGINT`, `SIGTERM`, `SIGHUP`, `SIGTSTP` |
| `RELOAD_CONFIG` | `SIGUSR1` |
| `NEED_REDRAW` | `SIGWINCH` |

---
</details>

<details>
<summary id="variables">Variables</summary>

## Variables

| Variable | Value |
|----------|-------|
| `$$` / `$PID` | Shell process ID |
| `$PPID` | Parent PID (from `/proc/self/stat`) |
| `$UID` | Real user ID |
| `$EUID` | Effective user ID |
| `$GID` | Real group ID |
| `$HOME` | Home directory |
| `$USER` | Username |
| `$HOSTNAME` | Hostname (env → `/etc/hostname` → `"localhost"`) |
| `$SHLVL` | Shell level (incremented once at startup) |
| `$SHELL` | Always `"context"` |
| `$PWD` | Current working directory |
| `$OLDPWD` | Previous working directory |
| `$?` | Last command exit status |
| `$!` | PID of most recent background command |
| `$0`–`$9` | Positional parameters |
| `$@` | All positional parameters (individually quoted when `$*` context) |
| `$*` | All positional parameters (single string with IFS first char) |
| `$#` | Number of positional parameters |
| `$-` | Current shell option flags |
| `_` | Last argument of previous command |
| `RANDOM` | Random integer 0–32767 |
| `SECONDS` | Seconds since shell started. Assignable to reset timer. |
| `LINENO` | Current line number in source file |
| `BASH_VERSION` | Always `"0.70.0"` |
| `BASH` | Always `"context"` |
| `BASH_REMATCH` | Array of regex match groups from `[[ =~ ]]` |
| `BASH_SOURCE` | Source file for current function/script |
| `FUNCNAME` | Name of current function (via call stack) |
| `BASH_ALIASES` | Associative array of defined aliases |
| `BASH_CMDS` | Hash table of command lookup cache |
| `PIPESTATUS` | Array of exit statuses from last pipeline |
| `DIRSTACK` | Directory stack (indexed array, also `DIRSTACK_0`..`DIRSTACK_N`) |
| `HISTCMD` | Current history number |
| `COPROC_PID` | PID of last coprocess |
| `COPROC` | Name of last coprocess |
| `GROUPS` | Array of groups current user belongs to |

---
</details>

<details>
<summary id="modules">Modules</summary>

## Modules

Modules are shell-script based, located at `~/.config/context/modules/<name>/`.

| File | Purpose |
|------|---------|
| `init.context` | Executed on `module load` |
| `fini.context` | Executed on `module unload` (reserved) |
| `README` | Module documentation |

| Command | Description |
|---------|-------------|
| `module load NAME` | Load module (source `init.context`) |
| `module unload NAME` | Unload module (reserved) |
| `module list` | List all modules |
| `module info NAME` | Show module info |

Environment changes from module scripts are merged back into the shell environment.

---
</details>

<details>
<summary id="coloring">Coloring</summary>

## Coloring

### Auto-Detection

| `$COLORTERM` | `$TERM` | Capability |
|---|---|---|
| `truecolor` or `24bit` | (anything) | TrueColor (24-bit ANSI) |
| (unset) | `*256color*` | 256 colors |
| (unset) | `dumb` or empty | No color |
| (unset) | anything else | 16 colors |

### Graceful Degradation

Any `#RRGGBB` hex color is converted to the best ANSI representation:

| Capability | Conversion | Example |
|------------|-----------|---------|
| TrueColor | `\x1b[38;2;R;G;Bm` | 24-bit direct color |
| 256 colors | `\x1b[38;5;Nm` | Nearest color index |
| 16 colors | `\x1b[3Xm` | Basic ANSI color |
| No color | `""` | Empty string |

---
</details>

<details>
<summary id="configuration">Configuration</summary>

## Configuration

### Config Locations

Search order (first match wins):

1. `~/.config/context/c.toml`
2. `~/.context/c.toml`
3. `~/.context/context.toml`
4. `~/.config/context/.ctxrc.toml`

Auto-created with full defaults on first run. Hot-reload via `kill -USR1 <pid>`.

### Config Schema (25 Sections, 300 Fields)

```
Config
├── history:        HistoryConfig         (11 fields)
├── cursor:         CursorConfig          (11 fields)
├── prompt:         PromptConfig          (51 fields)
├── box_config:     BoxConfig             (25 fields)
├── colors:         ColorsConfig          (30 fields)
├── symbols:        SymbolsConfig         (17 fields)
├── ascii:          AsciiConfig           (12 fields)
├── execution:      ExecutionConfig       (11 fields)
├── startup:        StartupConfig         (6 fields)
├── editor:         EditorConfig          (13 fields)
├── display:        DisplayConfig         (21 fields)
├── signals:        SignalsConfig         (8 fields)
├── environment:    EnvironmentConfig     (5 fields)
├── branding:       BrandingConfig        (7 fields)
├── autosuggest:    AutosuggestConfig     (8 fields)
├── keybindings:    KeybindingConfig      (bindings vec)
├── jobs:           JobControlConfig      (6 fields)
├── clipboard:      ClipboardConfig       (3 fields)
├── integration:    IntegrationConfig     (6 fields)
├── security:       SecurityConfig        (6 fields)
├── performance:    PerformanceConfig     (4 fields)
├── modes:          ModesConfig           (5 fields)
├── dynamic:        DynamicColorsConfig   (4 fields)
├── python:         PythonConfig          (6 fields)
└── named_dirs:     HashMap<String,String>
```

#### [history]

| Field | Default | Description |
|-------|---------|-------------|
| `max_size` | `256` | Max history entries |
| `file` | `"~/.ctx_history"` | History file path |
| `deduplicate` | `true` | Collapse consecutive duplicates |
| `ignore_space` | `true` | Ignore commands starting with space |
| `ignore_patterns` | `[]` | Regex patterns to ignore |
| `share_across_sessions` | `true` | Persist across sessions |
| `sync_on_command` | `true` | Sync from file on each prompt |
| `expire_days` | `0` | Auto-expire history entries (0 = never) |
| `save_on_every_command` | `false` | Write history file after every command |
| `search_case_sensitive` | `false` | Case-sensitive history search |
| `substring_search` | `true` | Substring matching in history search |

#### [cursor]

| Field | Default | Description |
|-------|---------|-------------|
| `symbol` | `"_"` | Cursor character |
| `blink` | `true` | Cursor blink via DECSCUSR |
| `color` | `"#6b7280"` | Cursor color (normal) |
| `color_error` | `"#ef4444"` | Cursor color (on error) |
| `style` | `"block"` | Cursor shape: `"block"`, `"beam"`, `"underline"` |
| `width` | `1` | Cursor width (reserved) |
| `format` | `[]` | Multi-line prompt format lines |
| `format_input_line` | `-1` | Which line receives input |
| `format_align` | `"left"` | Alignment: `"left"`, `"right"`, `"center"` |
| `format_padding` | `0` | Padding in chars |
| `format_colorize` | `true` | Apply cursor color to all format lines |

#### [prompt]

| Field | Default | Description |
|-------|---------|-------------|
| `top_line_left` | `"context"` | Left side of top line |
| `top_line_right` | `""` | Right side of top line |
| `color_cwd` | `"#9ca3af"` | CWD color |
| `color_prompt` | `"#22c55e"` | Prompt char color |
| `color_top` | `"#ffffff"` | Top line color |
| `color_symbol` | `"#22c55e"` | Symbol color |
| `color_user` | `"#d1d5db"` | User@host color |
| `color_host` | `"#9ca3af"` | Hostname color |
| `show_top_line` | `false` | Show top line |
| `gap_after_top` | `0` | Blank lines after top |
| `gap_after_middle` | `0` | Blank lines after middle |
| `show_user_host` | `false` | Show user@host |
| `user_host_format` | `"{user}@{host}"` | User@host format string |
| `cwd_max_depth` | `0` | Max path depth (0 = unlimited) |
| `show_git_branch` | `false` | Show git branch |
| `git_branch_color` | `"#f59e0b"` | Git branch color |
| `prompt_prefix` | `""` | Text before CWD |
| `prompt_suffix` | `"❯"` | Text after CWD |
| `newline_before_prompt` | `false` | Add blank line before prompt |
| `show_path` | `true` | Show path in prompt |
| `right_prompt` | `""` | Right-aligned prompt text |
| `right_prompt_color` | `"#6b7280"` | Right prompt color |
| `right_prompt_hide_threshold` | `0.8` | Hide when input > threshold × terminal width |
| `transient_prompt` | `false` | Replace prompt after execution |
| `transient_prompt_format` | `"{user}@{host} {cwd} {suffix} "` | Transient format |
| `async_prompt` | `false` | Cache prompt computation |
| `async_prompt_cache_ms` | `1000` | Cache duration in ms |
| `instant_prompt` | `false` | Show cached prompt on startup |
| `prompt_bold` | `false` | Bold prompt char |
| `gap_before_prompt` | `0` | Blank lines before prompt |
| `error_prompt_color` | `"#ef4444"` | Prompt char color on error |
| `prompt_color_success` | `"#22c55e"` | Prompt char color on success |
| `vi_prompt_insert` | `"-- INSERT --"` | Vi insert mode indicator |
| `vi_prompt_normal` | `"-- NORMAL --"` | Vi normal mode indicator |
| `vi_prompt_visual` | `"-- VISUAL --"` | Vi visual mode indicator |
| `vi_cmd_prompt` | `":"` | Vi command mode indicator (normal mode default) |
| `vi_cmd_color` | `"#22c55e"` | Vi normal mode indicator color |
| `vi_cmd_color_error` | `"#ef4444"` | Vi visual mode indicator color |
| `vi_cmd_color_success` | `"#22c55e"` | Vi insert mode indicator color |
| `prompt_eol_escape` | `"\\033[0m"` | ANSI reset appended at end of prompt line |
| `rprompt_eol_escape` | `"\\033[0m"` | ANSI reset appended at end of right prompt |
| `show_branch_only_when_dirty` | `false` | Only show git branch when repo is dirty |
| `git_dirty_char` | `"*"` | Character shown when branch is dirty |
| `git_clean_char` | `""` | Character shown when branch is clean |
| `git_staged_char` | `"+"` | Character shown for staged files |
| `git_untracked_char` | `"?"` | Character shown for untracked files |
| `git_ahead_char` | `"↑"` | Character shown when ahead of remote |
| `git_behind_char` | `"↓"` | Character shown when behind remote |
| `show_python_venv` | `true` | Show Python virtualenv name in prompt |
| `show_node_version` | `false` | Show Node.js version in prompt |
| `show_rust_version` | `false` | Show Rust toolchain version in prompt |

#### [box_config]

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `true` | Enable box rendering |
| `border_color` | `"#ffffff"` | Box border color |
| `corner_tl` | `"╭"` | Top-left corner |
| `corner_tr` | `"╮"` | Top-right corner |
| `corner_bl` | `"╰"` | Bottom-left corner |
| `corner_br` | `"╯"` | Bottom-right corner |
| `horizontal_char` | `"─"` | Horizontal line char |
| `vertical_char` | `"│"` | Vertical line char |
| `padding_left` | `1` | Left padding |
| `padding_right` | `1` | Right padding |
| `show_exit_code` | `true` | Show exit code in prompt |
| `exit_label` | `"exit"` | Exit label text |
| `exit_code_color` | `"#ef4444"` | Exit code color |
| `min_width` | `40` | Welcome box min width |
| `gap_before` | `0` | Gap before box |
| `gap_after` | `0` | Gap after prompt (blank lines below) |
| `title` | `""` | Box title |
| `title_color` | `"#d1d5db"` | Box title color |
| `content_color` | `"#f9fafb"` | Box content color |
| `border_style` | `"rounded"` | Border style: `"rounded"`, `"double"`, `"thick"`, `"none"` |
| `separator_char` | `"·"` | Separator character |
| `show_title_only_when_busy` | `false` | Only show title during command execution |
| `top_line_padding` | `1` | Spaces before top line label |
| `bottom_line_char` | `""` | Character for bottom border line |
| `box_width_mode` | `"terminal"` | Width mode: `"terminal"`, `"content"`, `"fixed"` |

#### [colors]

| Field | Default | Description |
|-------|---------|-------------|
| `primary` | `"#ffffff"` | Primary color + terminal foreground (OSC 10) |
| `accent` | `"#d1d5db"` | Accent color |
| `info` | `"#9ca3af"` | Info color |
| `success` | `"#22c55e"` | Success color |
| `err` | `"#ef4444"` | Error color |
| `warning` | `"#f59e0b"` | Warning color (used for warning symbol) |
| `dim` | `"#6b7280"` | Dim color |
| `text` | `"#f9fafb"` | Text color |
| `bold` | `"#ffffff"` | Bold color |
| `underline` | `"#d1d5db"` | Underline color |
| `reverse` | `"#111827"` | Reverse color |
| `bg_primary` | `"#111827"` | Terminal background (OSC 11) |
| `bg_err` | `"#3d0000"` | Background error |
| `bg_success` | `"#003d1a"` | Background success |
| `bg_warning` | `"#3d2e00"` | Background warning |
| `bg_info` | `"#1f2937"` | Background info |
| `gradient_start` | `"#ffffff"` | CWD gradient start (fallback) |
| `gradient_mid` | `"#9ca3af"` | CWD gradient mid (fallback) |
| `gradient_end` | `"#22c55e"` | CWD gradient end (fallback) |
| `rprompt_bg` | `""` | Right prompt background |
| `transient` | `"#6b7280"` | Transient prompt color |
| `cwd_gradient_start` | `"#d1d5db"` | CWD path gradient start |
| `cwd_gradient_end` | `"#9ca3af"` | CWD path gradient end |
| `syntax_comment` | `"#6b7280"` | Comment color in syntax highlighting |
| `syntax_string` | `"#f59e0b"` | String literal color |
| `syntax_variable` | `"#22d3ee"` | Variable color |
| `syntax_operator` | `"#22c55e"` | Operator color |
| `syntax_command` | `"#3b82f6"` | Command color |
| `syntax_flag` | `"#c084fc"` | Flag color (`--flag`, `-f`) |
| `syntax_path` | `"#fb923c"` | Path color (`/path`, `./rel`) |
| `syntax_number` | `"#22d3ee"` | Number color (`123`, `0xff`) |
| `syntax_use_dynamic_colors` | `true` | Derive all syntax colors from dynamic palette instead of hex above |

All accept any `#RRGGBB` hex value. Auto-degraded to terminal capability.

#### [symbols]

| Field | Default | Description |
|-------|---------|-------------|
| `prompt_char` | `"❯"` | Prompt symbol |
| `note_char` | `"▸"` | Note symbol |
| `exit_prefix` | `"exit"` | Exit prefix |
| `arrow_char` | `"→"` | Arrow |
| `separator_char` | `"·"` | Separator |
| `git_branch_char` | `"⌿"` | Git branch icon |
| `directory_char` | `" "` | Directory icon |
| `file_char` | `" "` | File icon |
| `executable_char` | `"*"` | Executable marker |
| `link_char` | `"@"` | Symlink marker |
| `pipe_char` | `"|"` | Pipe marker |
| `socket_char` | `"="` | Socket marker |
| `continuation_char` | `"·"` | Continuation prompt |
| `job_char` | `"&"` | Background job indicator |

`prompt_char` is used in the prompt. Others are available for prompt format strings.

> **Note:** Status symbols (`✓`/`✘`/`⚠`) are NOT auto-rendered in the prompt. Use `{success_char}`, `{error_char}`, or `{status_char}` in your format strings to display them. The prompt only renders what your config specifies.

#### [ascii]

| Field | Default | Description |
|-------|---------|-------------|
| `lines` | `[]` | Custom art lines (empty = built-in) |
| `file` | `""` | Load art from file (~ expanded, blank lines split multi-frame) |
| `blocks` | `[]` | Per-line color blocks for custom art |
| `show_in_box` | `false` | Wrap art in box border |
| `box_color` | `"#ffffff"` | Box border color |
| `margin_top` | `1` | Lines above art |
| `margin_bottom` | `1` | Lines below art |
| `margin_left` | `2` | Spaces before art |
| `center` | `false` | Center using terminal width |
| `color` | `"#8b5cf6"` | Default art color (reserved) |
| `animate` | `false` | Animate multi-frame art |
| `animate_delay_ms` | `200` | Delay between frames |

#### [execution]

| Field | Default | Description |
|-------|---------|-------------|
| `shell` | `"/bin/bash"` | Fallback shell (reserved) |
| `timeout_seconds` | `0` | Exec timeout (reserved) |
| `fork_method` | `"fork"` | Fork method (reserved) |
| `umask` | `"0022"` | File mode creation mask |
| `strip_env_on_exec` | `false` | Strip env vars in children |
| `path_override` | `""` | Custom PATH for `find_in_path` |
| `cdspell` | `false` | Suggest corrections on cd typos (Levenshtein) |
| `float_precision` | `6` | Decimal places for `math` builtin |
| `bash_compat` | `false` | Basic bash compatibility mode |
| `max_forks_per_command` | `128` | Max forks per command pipeline |
| `exit_on_pipefail` | `false` | Exit on pipe failure |

#### [startup]

| Field | Default | Description |
|-------|---------|-------------|
| `show_welcome` | `true` | Show ASCII art on start |
| `welcome_message` | `""` | Custom welcome message |
| `welcome_color` | `"#f9fafb"` | Welcome message color |
| `run_commands` | `[]` | Commands to run on startup |
| `startup_delay_ms` | `0` | Delay after welcome display (ms) |
| `show_config_path` | `false` | Print config path on exit |

#### [editor]

| Field | Default | Description |
|-------|---------|-------------|
| `mode` | `"emacs"` | Keybinding mode: `"emacs"`, `"vi"` |
| `bell` | `"visible"` | Bell behavior: `"visible"`, `"audible"`, `"none"` |
| `auto_cd` | `true` | Auto cd on directory name |
| `expand_aliases` | `true` | Expand aliases before exec |
| `colorize_output` | `true` | Colorize command output |
| `max_line_length` | `4096` | Max chars on input line |
| `word_delimiters` | `" \t\n\|&;><\`(){}[]$'\"\\"` | Ctrl+W word delimiters |
| `bracketed_paste` | `true` | Enable bracketed paste mode |
| `auto_match_quotes` | `false` | Auto-close matching quotes |
| `vi_cursor_block` | `"block"` | Vi normal mode cursor shape |
| `vi_cursor_insert` | `"beam"` | Vi insert mode cursor shape |
| `emacs_overwrite_mode` | `false` | Emacs overwrite/readline mode |
| `hide_cursor_on_exec` | `true` | Hide terminal cursor while external commands run |

The Vi mode enum has three variants: `ViInsert`, `ViNormal`, `ViVisual`.
When `editor.mode = "vi"`, vi mode indicators are shown via prompt config fields
(`vi_prompt_insert`, `vi_prompt_normal`, `vi_prompt_visual`, `vi_cmd_prompt`).

#### [display]

| Field | Default | Description |
|-------|---------|-------------|
| `show_exit_code_on_error_only` | `true` | `{exit_code}` only shown when status != 0 |
| `compact_mode` | `false` | Compact prompt output |
| `color_mode` | `"true_color"` | Color depth: `"true_color"`, `"256"`, `"16"`, `"none"` (auto-detected) |
| `utf8_mode` | `"full"` | UTF-8 level: `"full"`, `"basic"`, `"ascii"` |
| `show_command_duration` | `false` | Show command duration in prompt |
| `duration_color` | `"#6b7280"` | Duration text color |
| `show_timestamp` | `false` | `{time}` and `{time_full}` expanded only when true |
| `timestamp_format` | `"%H:%M:%S"` | Timestamp format |
| `timestamp_color` | `"#6b7280"` | Timestamp text color |
| `show_pid` | `false` | `{pid}` expanded only when true |
| `status_line` | `false` | Renders secondary line below prompt |
| `status_line_format` | `"{cwd} \| {exit_code} \| {duration}"` | Status line format |
| `title_bar` | `false` | Renders bordered title bar above prompt |
| `title_bar_format` | `"context — {cwd}"` | Title bar format |
| `title_bar_color` | `"#6b7280"` | Title bar color |
| `show_session_info` | `false` | Show session info on prompt |
| `session_info_format` | `"context {version} — {cwd}"` | Session info format |
| `powerline_symbols` | `false` | Use powerline glyphs for separators |
| `nerd_fonts` | `false` | Use nerd font icons |
| `show_command_summary` | `false` | Show `[<status>] <command> (exit $)` after execution |
| `command_summary_format` | `"[{status_char}] {command} (exit {exit_code})"` | Command summary format |

#### [signals]

| Field | Default | Description |
|-------|---------|-------------|
| `sigint_action` | `"cancel_line"` | SIGINT behavior: `"cancel_line"`, `"ignore"`, `"default"` |
| `sigquit_action` | `"ignore"` | SIGQUIT behavior: `"ignore"`, `"suspend"` |
| `sigtstp_action` | `"suspend"` | SIGTSTP behavior: `"suspend"`, `"ignore"` |
| `sigwinch_action` | `"redraw"` | SIGWINCH behavior: `"redraw"`, `"ignore"` |
| `forward_signals_to_child` | `true` | Forward signals to child processes |
| `reset_terminal_on_exit` | `true` | Disable raw mode, show cursor on exit |
| `sigusr1_action` | `"reload_config"` | SIGUSR1 behavior: `"reload_config"` (sets RELOAD_CONFIG + trap), `"ignore"` |
| `sigusr2_action` | `"ignore"` | SIGUSR2 behavior: `"reload_config"`, `"ignore"` |

#### [environment]

| Field | Default | Description |
|-------|---------|-------------|
| `passthrough` | `[]` | Vars to pass to child processes |
| `filter` | `[]` | Vars to filter from child processes |
| `set_defaults` | `["TERM=xterm-256color"]` | Applied on executor startup |
| `strip_on_exit` | `[]` | Vars to unset in children |
| `inherit_parent` | `true` | Inherit parent environment variables |

#### [branding]

| Field | Default | Description |
|-------|---------|-------------|
| `app_name` | `"context"` | ASCII logo to show |
| `version` | `env!("CARGO_PKG_VERSION")` | Version string (from Cargo.toml) |
| `shell_name` | `"context"` | Terminal title (`ESC]0;{shell_name}\x07`) |
| `author` | `""` | Author name |
| `tagline` | `"a fast, fully configurable POSIX shell"` | Tagline |
| `show_version_on_start` | `false` | Print version in welcome output |
| `show_config_path_on_start` | `false` | Print config path on exit |

#### [autosuggest]

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `true` | Enable/disable autosuggestions |
| `min_chars` | `1` | Min input chars before suggesting |
| `strategy` | `"history"` | Source strategy: `"history"` |
| `highlight_color` | `"#6b7280"` | Suggestion text color |
| `accept_key` | `"Right"` | Accept full suggestion key |
| `accept_word_key` | `"Alt+Right"` | Accept word key |
| `max_suggestions` | `50` | Max suggestions to show |
| `case_sensitive` | `false` | Case-sensitive matching |

#### [keybindings]

Custom key bindings with widget dispatch.

```toml
[keybindings]
bindings = [
  { key = "Ctrl+R", widget = "HistorySearch" },
  { key = "Ctrl+A", widget = "MoveToLineStart" },
  { key = "Ctrl+E", widget = "MoveToLineEnd" },
  { key = "Up", widget = "HistorySearchUp" },
  { key = "Down", widget = "HistorySearchDown" },
  { key = "Ctrl+L", widget = "ClearScreen" },
]
```

Available widgets: `HistorySearch`, `HistorySearchUp`, `HistorySearchDown`,
`AcceptSuggestion`, `AcceptSuggestionWord`, `MoveToLineStart`, `MoveToLineEnd`,
`MoveWordLeft`, `MoveWordRight`, `DeleteWordLeft`, `DeleteWordRight`,
`ClearScreen`, `Redraw`, `Undo`, `Redo`, `Yank`, `KillLine`, `TransposeChars`.

#### [jobs]

| Field | Default | Description |
|-------|---------|-------------|
| `notify_on_job_done` | `false` | Print status on job completion |
| `auto_report_stopped` | `true` | Auto-report stopped PIDs |
| `max_jobs` | `128` | Max concurrent jobs |
| `warn_on_suspended` | `true` | Warn if suspended jobs on exit |
| `done_format` | `"[done] {command} (exit {exit_code})"` | Done notification format |
| `stopped_format` | `"[stopped] {command} pid={pid}"` | Stopped notification format |

#### [clipboard]

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `false` | Enable clipboard integration |
| `method` | `"auto"` | Clipboard method: `"auto"`, `"osc52"`, `"xclip"`, `"pbcopy"` |
| `yank_to_clipboard` | `false` | Auto-yank killed text to clipboard |

#### [integration]

| Field | Default | Description |
|-------|---------|-------------|
| `enable_fzf` | `false` | Enable fzf key bindings |
| `enable_zoxide` | `false` | Enable zoxide integration |
| `fzf_key_bindings` | `true` | fzf Ctrl+T/Ctrl+R/Ctrl+D bindings |
| `zoxide_init` | `true` | Init zoxide on startup |
| `starship_prompt` | `false` | Use starship prompt |

#### [security]

| Field | Default | Description |
|-------|---------|-------------|
| `restricted_mode` | `false` | Restrict commands: blocks `exec`, `eval`, `source`, `.`, `kill`, `env`, `export`, `bash`, `sh`, `zsh`, `fish`, `command`, `builtin`, `enable`, `hash`; blocks `cd ..` and `cd /` |
| `no_exec_commands` | `[]` | Additional commands to block |
| `sanitize_path` | `true` | Remove empty entries from PATH |
| `mask_secrets` | `false` | Mask env var values containing passwords |
| `audit_log` | `false` | Log all commands to file |
| `audit_log_path` | `"~/.ctx_audit.log"` | Audit log path |

#### [performance]

| Field | Default | Description |
|-------|---------|-------------|
| `max_history_load_lines` | `100000` | Max lines loaded from history file |

#### [modes]

High-level mode toggles that set multiple fields at once.

| Field | Default | Description |
|-------|---------|-------------|
| `symbol_mode` | `"unicode"` | `"nerd"`, `"unicode"`, `"ascii"` — sets prompt/symbols/box chars |
| `border_mode` | `"unicode"` | Override for `box_config.border_style` |
| `nerd_mode` | `"off"` | Global nerd font toggle: `"on"`, `"off"` (sets `display.nerd_fonts`) |
| `powerline_mode` | `"off"` | Global powerline toggle: `"on"`, `"off"` (sets `display.powerline_symbols`) |
| `color_depth` | `"true_color"` | Override for `display.color_mode` |

#### [dynamic]

Wallpaper-based dynamic color extraction powered by wallust. Extracts a 16-color
palette from your wallpaper using multiple backends (k-means, SIMD fast_resize,
histogram) and maps them to the entire shell color scheme. Works with swww,
sway, GNOME, or auto-detects from common wallpaper paths.

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `false` | Enable dynamic color extraction |
| `wallpaper_path` | `null` | Path to wallpaper image (auto-detect if null) |
| `fallback_to_terminal_bg` | `true` | Fall back to OSC 11 terminal background query |
| `color_mapping` | `{}` | Override specific color slots |

```toml
[dynamic]
enabled = true
# wallpaper_path = "~/wallpapers/mountains.jpg"
fallback_to_terminal_bg = true

[dynamic.color_mapping]
# Override specific colors (hex without #)
success = "22c55e"    # force a specific green
```

**Color slot mapping** (derived from wallust's 16-color palette):

| Slot | wallust source | Description |
|------|----------------|-------------|
| `bg_primary` | `background` | Terminal background |
| `primary` | `foreground` | Main foreground |
| `accent` | `color6` (cyan) | Accent color |
| `success` | `color2` (green) | Success indicators |
| `err` | `color1` (red) | Error indicators |
| `warning` | `color3` (yellow) | Warning indicators |
| `info` | `color4` (blue) | Info indicators |
| `dim` | darkened foreground | Dimmed text |

All prompt colors, box border colors, and display colors are
automatically derived from the extracted palette when dynamic is enabled.

#### [python]

Embedded Python engine via PyO3. Write themes, plugins, and full TUI tools as
single `.py` files. Any Python library works: rich, textual, npyscreen, blessed,
urwid, prompt_toolkit, asciimatics, etc. Venv support included.

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `false` | Enable Python engine |
| `theme` | `""` | Theme name or path to a single `.py` theme file. A name is resolved via `t.desc`; empty uses a `t.desc` entry as the default |
| `tui` | `""` | TUI name or path to a single `.py` TUI file. A name is resolved via `t.desc`; empty uses a `t.desc` entry as the default |
| `plugins` | `[]` | List of paths to `.py` plugin files |
| `fallback_on_error` | `true` | Fall back to native if Python fails |
| `venv_path` | `""` | Path to venv — auto-activates before loading theme/plugins |
| `tui_mode` | `false` | Hand terminal to theme's `run()` function |

```toml
[python]
enabled = true
theme = "context"
plugins = [
    "~/.config/context/plugins/git_status.py",
    "~/.config/context/plugins/weather.py",
]
venv_path = "~/.venvs/context"
tui_mode = false
```

Themes and TUIs can be registered in a `t.desc` TOML file, loaded from the first existing, parseable file among `~/.config/context/t.desc`, `/etc/context/t.desc`, `./t.desc`, or `<cwd>/t.desc` (files are never merged; `path` values support `~` expansion) with
`[theme.<id>]` / `[tui.<id>]` sections containing `name`, `path`, and optional
`description`. When `theme` or `tui` is a registered name it is resolved to the
matching `path`; an empty value uses a `t.desc` entry as the default.

See [**`[Python]`**](#python-integration) for full API docs.

### Hot-Reload

```sh
kill -USR1 $(pgrep context)
```

Re-reads config file. Next prompt cycle uses new settings. No restart required.

---
</details>

<details>
<summary id="python">Python</summary>

## Python

Embedded Python via PyO3. Any Python library works: rich, textual, npyscreen,
blessed, urwid, prompt_toolkit, asciimatics, etc.

One single `.py` file = one theme. One single `.py` file = one plugin. No
multi-file setup needed. Everything goes in one file.

### Themes

A theme file defines how the prompt looks. Context calls your functions with the
full shell Context as keyword arguments. Return whatever you want. The theme engine
loads the file as a Python module and calls `render_prompt(**context)`, then
`render_right_prompt(**context)` and `render_command_summary(**context)` when the
theme defines them.

```python
# ~/.config/context/theme.py
def render_prompt(**context):
    cwd = context.get("cwd", "~")
    git = context.get("git_branch", "")
    exit_code = int(context.get("exit_code", "0"))
    status = "\u2714" if exit_code == 0 else "\u2718"
    return {
        "lines_above": [f" ({git})"] if git else [],
        "input_prefix": f"{cwd} \u276f ",
        "right_prompt": f"{status}",
        "colors": {"accent": "#3b82f6", "cwd": "#a78bfa"},
    }
```

**Context dict keys** (all env vars available, prefixed with `env_`):

| Key | Value |
|-----|-------|
| `cwd` | Current working directory |
| `user` | Username |
| `host` | Hostname |
| `exit_code` | Last command exit status |
| `git_branch` | Current git branch |
| `shell_version` | Context version |
| `shell_name` | Shell name from branding |
| `terminal_width` | Terminal width in columns |
| `terminal_height` | Terminal height in rows |
| `env_TERM`, `env_PATH`, `env_HOME`, ... | All env vars with `env_` prefix |

**Return dict keys** (all optional, return what you need):

| Key | Type | Description |
|-----|------|-------------|
| `lines_above` | `list[str]` | Lines above the input prompt |
| `input_prefix` | `str` | Text before the cursor |
| `right_prompt` | `str` | Right-side prompt text |
| `colors` | `dict` | Color map (any keys, Context reads `accent`, `success`, `error`, `primary`, `dim`, `text`, `cwd`) |
| *(any other keys)* | `str` | Stored in extra dict, accessible later |

Or just return a string — Context uses it as `input_prefix`.

### Plugins

A plugin file can define any public functions. Context auto-discovers ALL of them
as hooks. No hardcoded hook list — define whatever functions you want.

```python
# ~/.config/context/plugins/my_tool.py
import time

_count = 0

def on_startup():
    print("shell started!", flush=True)

def on_postexec(command, exit_code, duration_ms="", cwd="", **env):
    global _count
    _count += 1

def on_exit():
    print(f"\ntotal commands: {_count}", flush=True)

def my_custom_hook(**kwargs):
    """Any function name works."""
    pass
```

**Auto-discovered hooks** (define any or none):

| Hook | Args | When |
|------|------|------|
| `on_startup()` | — | Shell started |
| `on_exit()` | — | Shell exiting |
| `on_preexec(command=)` | command | Before command runs |
| `on_postexec(command=, exit_code=, duration_ms=, cwd=, **env)` | all shell state | After command runs |
| `on_precmd()` | — | Before prompt displays |
| `on_postcmd()` | — | After command completes |
| `on_command_not_found(command=)` | command | Command not found |
| `on_prompt(cwd=, exit_code=, **env)` | full context | During prompt render |
| `on_keypress(key=)` | key pressed | On keypress in editor |

### TUI Mode

Any theme or plugin can define `run()` to take over the terminal completely:

```python
# With rich:
from rich.console import Console
from rich.table import Table
def run():
    console = Console()
    table = Table(title="My TUI Tool")
    table.add_column("Name", style="cyan")
    table.add_column("Value", style="green")
    table.add_row("Status", "Running")
    console.print(table)
    input("Press Enter to exit...")
    return True

# With textual:
from textual.app import App
class MyApp(App):
    CSS = "Screen { background: $surface }"
    def compose(self):
        yield Label("My Context TUI")
def run():
    MyApp().run()
    return True
```

Set in c.toml:

```toml
[python]
enabled = true
theme = "~/.config/context/theme.py"
tui_mode = true
```

When `tui_mode = true` and your theme defines `run()`:
1. Context initializes config, signals, executor
2. Disables raw mode, shows cursor
3. Calls your `run()` function
4. When `run()` returns, Context exits

### Virtual Environments

**Option 1: config-level venv**

```toml
[python]
venv_path = "~/.venvs/context"
```

context auto-detects the Python version inside the venv and adds its
`site-packages` to `sys.path` before loading any theme or plugin.

**Option 2: inline venv in your .py file**

```python
import sys, os
venv = os.path.expanduser("~/.venvs/myproject")
site = os.path.join(venv, "lib", f"python{sys.version_info.major}.{sys.version_info.minor}", "site-packages")
if os.path.isdir(site):
    sys.path.insert(0, site)

# Now any library is available:
import rich
from textual.app import App
import npyscreen
```

Both approaches work with any library. Install whatever you need in your
venv: `pip install rich textual npyscreen blessed urwid prompt_toolkit`.

### Isolation

The Python subsystem is fully isolated:
- `catch_unwind` at every boundary — if Python panics, Context falls back silently
- `fallback_on_error = true` (default) — theme/plugin errors don't crash context
- If Python is unavailable, Context uses its native Rust prompt

---
</details>

<details>
<summary id="jobs">Jobs</summary>

## Jobs

```sh
$ sleep 10 &              # background job
$ jobs                    # list all jobs
$ fg %1                   # bring job 1 to foreground
$ bg %1                   # resume job 1 in background
```

| Job Type | Steps |
|----------|-------|
| Background | `setsid()` → new session, `setpgid(0, 0)` → new process group, `SIGHUP` ignored |
| Foreground | `setpgid(pid, pid)` → own process group, `tcsetpgrp(pid)` → child gets terminal, `waitpid(pid)` → block, `tcsetpgrp(self)` → shell regains terminal |

---
</details>

<details>
<summary id="trapping">Trapping</summary>

## Trapping

Trap handlers stored in `Env.traps`.

| Usage | Description |
|-------|-------------|
| `trap 'echo SIGINT received' SIGINT` | Register signal handler |
| `trap 'echo ERR' ERR` | Run on any command failure (exit code != 0) |
| `trap 'echo DEBUG' DEBUG` | Run before each command is executed |
| `trap 'echo RETURN' RETURN` | Run when a function or source returns |
| `trap '' SIGINT` | Ignore signal |
| `trap - SIGINT` | Reset to default |
| `trap` | Print all active traps |
| `trap -p SIGNAL` | Print handler for specific signal |

Supported signals: `SIGINT`, `SIGTERM`, `SIGHUP`, `SIGTSTP`, `SIGUSR1`, `SIGUSR2`.
Pseudo-signals: `ERR`, `DEBUG`, `RETURN`, `EXIT`.

Signal arrives → `TRAP_SIGNAL` set → main loop checks flag → looks up handler → tokenizes, parses, executes → flag reset.

`ERR` trap fires when a command returns non-zero (unless `set -e` causes immediate exit).
`DEBUG` trap fires before each simple command.
`RETURN` trap fires when a function or `source` completes.

---
</details>

## Credits

**`[Context]`** is part of the **`[Cudane]`** ecosystem.

- **`[Cudane]`** — The Distribution.
- **`[MCX]`** — Package Manager.

## Man Page

A full man page is included at `ctx.1`. Install with:

```sh
install -Dm644 ctx.1 /usr/share/man/man1/ctx.1
```

Or view directly:

```sh
man ./ctx.1
```

## License

**MIT License** ─ See [**`[LICENSE]`**](https://github.com/Mapuse/.github/blob/profile/LICENSE) for More Details.
