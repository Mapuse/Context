```shell
 ██████╗ ██████╗ ███╗   ██╗████████╗███████╗██╗  ██╗████████╗               
██╔════╝██╔═══██╗████╗  ██║╚══██╔══╝██╔════╝╚██╗██╔╝╚══██╔══╝               
██║     ██║   ██║██╔██╗ ██║   ██║   █████╗   ╚███╔╝    ██║                  
██║     ██║   ██║██║╚██╗██║   ██║   ██╔══╝   ██╔██╗    ██║                  
╚██████╗╚██████╔╝██║ ╚████║   ██║   ███████╗██╔╝ ██╗   ██║                  
 ╚═════╝ ╚═════╝ ╚═╝  ╚═══╝   ╚═╝   ╚══════╝╚═╝  ╚═╝   ╚═╝
```

`▐▀` `-` `▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▌`

- **`The Shell of Cudane`** — `v0.70.0`

`▐▄` `-` `▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌`

<details>

<summary>Contents</summary>

```
├── Overview
├── Build & Install
├── Usage
├── File Locations
├── Architecture
│   ├── Source Layout
│   └── Data Flow Pipeline
├── Shell Syntax
│   ├── Simple Commands
│   ├── Pipelines
│   ├── Logical Operators
│   ├── Redirects
│   ├── Variable Expansion
│   ├── Parameter Expansion Extras
│   ├── Command Substitution
│   ├── Arithmetic Expansion
│   ├── Floating Point Math
│   ├── Tilde Expansion
│   ├── Brace Expansion
│   ├── Glob Expansion
│   ├── ANSI-C Quoting
│   ├── Regex Matching
│   ├── [[ ]] Test Expressions
│   ├── Associative Arrays
│   └── Control Flow
├── Builtins Reference
│   ├── All Builtins
│   ├── Builtin Docs
│   └── Test / [ Operators ]
├── Autosuggestions
├── History
├── Editor & Keybindings
│   ├── Line Editor
│   ├── Emacs Mode (default)
│   ├── Vi Mode
│   ├── Kill Ring & Undo/Redo
│   ├── Syntax Highlighting
│   └── Custom Widgets
├── Prompt System
│   ├── PromptDisplay Structure
│   ├── Dynamic Variables
│   ├── Format Fields (Multi-Line Support)
│   ├── Right Prompt (RPROMPT)
│   ├── Transient Prompt
│   ├── Instant Prompt
│   └── Async Prompt
├── Signal Handling
├── Environment Variables
├── Module System
├── Color System
├── Configuration
│   ├── Config File Locations
│   └── Config Schema (25 Sections, 297 Fields)
├── Python Integration
│   ├── Themes
│   ├── Plugins
│   ├── TUI Mode
│   └── Virtual Environments
├── Job Control
├── Trap System
└── Test Suite
```

---

</details>

<details>

<summary>Overview</summary>

## Overview

| Category | What |
|----------|------|
| Lexer | `$'...'` ANSI-C quoting, `[[`/`]]` tokens, `>>\|` clobber |
| Shell syntax | `if`/`for`/`while`/`case`/functions/subshells/brace groups |
| Builtins | 51 builtins (see Builtins Reference) |
| Variable expansion | `${var:-def}`, `${var//old/new}`, `${var:=val}`, `${#var}`, `${##var}` |
| Parameter extras | `${var:u}` uppercase, `${var:l}` lowercase, `${var:r}` strip ext, `${var:e}` ext only, `${var:t}` basename, `${var:h}` dirname |
| Arithmetic | `let`, `(( ))`, `math` (float), operator precedence, parentheses |
| Floating point | `math` builtin, configurable precision, scientific notation |
| Regex | `regexmatch` builtin, captures into `$MATCH`, `$MATCH_0`, etc. |
| Associative arrays | `declare -A`, `${name[key]}`, `${(k)name}`, `${(v)name}`, `${(kv)name}` |
| Brace expansion | `echo {a,b,c}`, `echo {1..5}` |
| Glob expansion | `*`, `?`, `[abc]`, `[a-z]`, `[!abc]` |
| Multi-line input | Backslash `\` continuation, unclosed quotes, unclosed `$()` — shows `> ` continuation prompt |
| Editor modes | Emacs mode with kill ring, undo/redo. Vi insert/normal mode with motions, operators, registers |
| Word splitting | IFS-based splitting respects quoting context |
| `[[ ]]` | Full test expressions with pattern matching, regex, grouping, logical operators |
| Plugins | Shell-script plugins loaded from `~/.config/context/plugins/` |
| Autosuggestions | Fish-style grey suggestions from history |
| History | Shared across sessions via `flock`, `sync_history` on each prompt |
| Named directories | `hash -d name=path`, `~name` expands to path |
| Prompt | Multi-line cursor format, right prompt, transient, instant, async |
| Syntax highlighting | Real-time in the line editor |
| Color system | Auto-detect: TrueColor → 256 → 16 → None. Any `#RRGGBB` hex. Wallpaper-based dynamic colors |
| TOML config | 25 sections, 297 fields |
| Job control | `fg`, `bg`, `jobs`, `&`, process groups |
| Signal traps | `trap 'cmd' SIGINT` |
| Modules | `module load/unload/list/info` |
| Bash compat | `bash_compat` flag for basic compatibility |

---

</details>

<details>

<summary>Build & Install</summary>

## Build & Install

**Requirements:** `rustc 1.79+` (nightly recommended), `cargo`

| Command | Description |
|---------|-------------|
| `cargo build` | Build (debug) |
| `cargo build --release` | Build (release, optimized) |
| `cargo test` | Run all tests |
| `cargo clippy` | Lint |

Binary output: `./target/release/context`

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
| pyo3 | 0.25 | Embedded Python engine (themes, plugins, TUI) |

---

</details>

<details>

<summary>Usage</summary>

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
| `-V` | `--verbose-version` | Show detailed version, build info, and arch |
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
| `-b` | `--bash-compat` | Enable bash compatibility shims |

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

#### Job Control

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

<summary>File Locations</summary>

## File Locations

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

---

</details>

<details>

<summary>Architecture</summary>

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

<summary>Shell Syntax</summary>

## Shell Syntax

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

---

</details>

<details>

<summary>Builtins Reference</summary>

## Builtins Reference

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
| `history [N]` | Show last N entries. `-c` clears. |
| `set [NAME=VAL]` | Set shell variable. `-e`, `-u`, `-x`, `-a` options. |
| `env` | Print all variables as NAME=VALUE |
| `pwd` | Print working directory |
| `type NAME` | Check if command is builtin or external |
| `which NAME` | Print path to command |
| `echo [-neE] [args]` | Print args. `-n` no newline. `-e` process escapes. `-E` disable escapes. |
| `printf FMT [args]` | POSIX printf |
| `test EXPR` / `[ EXPR ]` | Conditional test |
| `let NAME=EXPR` | Arithmetic evaluation |
| `(( EXPR ))` | Arithmetic evaluation (syntactic sugar) |
| `math EXPR` | Floating point arithmetic |
| `regexmatch STR PAT [VAR]` | Regex match, captures into `$MATCH`, `$MATCH_0`, `$MATCH_1`, etc. |
| `exec CMD [args]` | Replace shell with CMD |
| `trap 'CMD' SIG` | Set signal handler |
| `pushd [dir]` | Push current dir, cd to dir. Supports `+n`/`-n` rotation. |
| `popd` | Pop and cd to top. Supports `+n`/`-n` removal. |
| `dirs [-v] [-l] [-p]` | Print directory stack. `-v` numbered, `-l` long paths, `-p` one-per-line. |
| `readonly NAME` | Mark variable as read-only |
| `declare [-x] [-r] [-A] NAME=VAL` | Declare variable with attributes |
| `hash -d name=path` | Define named directory. `hash -d name` unset. |
| `builtin CMD` | Verify CMD is a builtin |
| `shopt [-s] [-u] [opt]` | Set/unset/query shell options |
| `jobs` | List background jobs |
| `fg [N]` | Bring job N to foreground |
| `bg [N]` | Resume job N in background |
| `wait [PID]` | Wait for child process |
| `kill [-SIG] PID` | Send signal to process |
| `umask [MASK]` | Get/set file mode creation mask |
| `command [-p] CMD` | Execute CMD ignoring aliases. `-v` prints path. |
| `eval STRING` | Evaluate STRING as shell commands |
| `select NAME [in ITEMS]` | Interactive menu selection |
| `getopts OPTSTRING NAME [ARGS]` | Parse positional options |
| `read [-r] [-p PROMPT] [-s] [-a NAME] [-d DELIM] [-t TIMEOUT]` | Read line from stdin into variable(s) |
| `local NAME=VAL` | Declare local variable in function scope |
| `module load/unload/list/info NAME` | Load/unload/list/info modules |
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

### Test / [ Operators ]

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

---

</details>

<details>

<summary>Autosuggestions</summary>

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

<summary>History</summary>

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

<summary>Editor & Keybindings</summary>

## Editor & Keybindings

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
| Tab | Insert tab character |
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

**Vi Insert Mode** — Same as Emacs mode (Ctrl+C clears, Ctrl+D exits, Ctrl+R searches, etc.)

**Vi Normal Mode** — Press `Esc` from insert mode:

| Key | Action |
|-----|--------|
| `h` / `l` | Move left / right |
| `0` / `$` | Move to start / end of line |
| `w` / `b` | Move word forward / backward |
| `i` / `a` | Insert before / after cursor |
| `I` / `A` | Insert at start / end of line |
| `o` / `O` | Open line below / above |
| `x` | Delete character at cursor |
| `D` | Kill to end of line |
| `dd` | Kill entire line |
| `p` | Paste from kill ring |
| `u` | Undo |
| `r` | Redo |
| `c` | Cancel (clear input, back to insert) |
| `v` | Enter visual mode |
| `Esc` | Return to insert mode |

### Kill Ring & Undo/Redo

- **Kill ring**: `Ctrl+K`, `Ctrl+U`, `Ctrl+W` in Emacs mode save killed text. `Ctrl+Y` yanks the most recent kill.
- **Undo/Redo**: `Ctrl+Z` undoes the last edit. `Ctrl+_` redoes. Both work in Emacs and Vi modes.
- **Vi kill ring**: `x`, `D`, `dd` in Vi normal mode also save to the kill ring.

### Syntax Highlighting

| Element | ANSI Color |
|---------|------------|
| Commands (first word) | Bold blue `\x1b[1;34m` |
| `$variables` | Cyan `\x1b[36m` |
| `"double-quoted strings"` | Yellow `\x1b[33m` |
| `'single-quoted strings'` | Yellow `\x1b[33m` |
| Operators `\|` `&` `;` `>` `<` | Green `\x1b[32m` |
| Comments `#...` | Gray `\x1b[90m` |
| Escape sequences `\x` | Green `\x1b[32m` |

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

<summary>Prompt System</summary>

## Prompt System

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
    "  ╔═══════════════════════╗",
    "  ║  Welcome to {app_name}  ║",
    "  ║  {version}             ║",
    "  ╚═══════════════════════╝"
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

<summary>Signal Handling</summary>

## Signal Handling

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

<summary>Environment Variables</summary>

## Environment Variables

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
| `$@` | All positional parameters |
| `$*` | All positional parameters |
| `$#` | Number of positional parameters |

---

</details>

<details>

<summary>Module System</summary>

## Module System

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

<summary>Color System</summary>

## Color System

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

<summary>Configuration</summary>

## Configuration

### Config File Locations

Search order (first match wins):

1. `~/.config/context/c.toml`
2. `~/.context/c.toml`
3. `~/.context/context.toml`
4. `~/.config/context/.ctxrc.toml`

Auto-created with full defaults on first run. Hot-reload via `kill -USR1 <pid>`.

### Config Schema (25 Sections, 297 Fields)

```
Config
├── history:        HistoryConfig         (11 fields)
├── cursor:         CursorConfig          (11 fields)
├── prompt:         PromptConfig          (51 fields)
├── box_config:     BoxConfig             (25 fields)
├── colors:         ColorsConfig          (28 fields)
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
| `syntax_use_dynamic_colors` | `true` | Derive all syntax colors from dynamic palette instead of hex above |

All accept any `#RRGGBB` hex value. Auto-degraded to terminal capability.

#### [symbols]

| Field | Default | Description |
|-------|---------|-------------|
| `prompt_char` | `"❯"` | Prompt symbol |
| `note_char` | `"▸"` | Note symbol |
| `error_char` | `"✘"` | Error symbol |
| `exit_prefix` | `"exit"` | Exit prefix |
| `success_char` | `"✓"` | Success symbol |
| `warning_char` | `"⚠"` | Warning symbol |
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

`prompt_char` and `error_char` are used in the prompt. Others are available for prompt format strings.

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
| `theme` | `""` | Path to a single `.py` theme file |
| `plugins` | `[]` | List of paths to `.py` plugin files |
| `fallback_on_error` | `true` | Fall back to native if Python fails |
| `venv_path` | `""` | Path to venv — auto-activates before loading theme/plugins |
| `tui_mode` | `false` | Hand terminal to theme's `run()` function |

```toml
[python]
enabled = true
theme = "~/.config/context/theme.py"
plugins = [
    "~/.config/context/plugins/git_status.py",
    "~/.config/context/plugins/weather.py",
]
venv_path = "~/.venvs/context"
tui_mode = false
```

See [Python Integration](#python-integration) for full API docs.

### Hot-Reload

```sh
kill -USR1 $(pgrep context)
```

Re-reads config file. Next prompt cycle uses new settings. No restart required.

---

</details>

<details>

<summary>Python Integration</summary>

## Python Integration

Embedded Python via PyO3. Any Python library works: rich, textual, npyscreen,
blessed, urwid, prompt_toolkit, asciimatics, etc.

One single `.py` file = one theme. One single `.py` file = one plugin. No
multi-file setup needed. Everything goes in one file.

### Themes

A theme file defines how the prompt looks. Context calls your functions with the
full shell Context as keyword arguments. Return whatever you want.

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

<summary>Job Control</summary>

## Job Control

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

<summary>Trap System</summary>

## Trap System

Trap handlers stored in `Env.traps`.

| Usage | Description |
|-------|-------------|
| `trap 'echo SIGINT received' SIGINT` | Register handler |
| `trap '' SIGINT` | Ignore signal |
| `trap - SIGINT` | Reset to default |
| `trap` | Print all active traps |

Supported: `SIGINT`, `SIGTERM`, `SIGHUP`, `SIGTSTP`.

Flow: signal arrives → `TRAP_SIGNAL` atomic set → main loop checks flag → looks up handler → tokenizes, parses, executes → flag reset.

---

</details>

<details>

<summary>Test Suite</summary>

## Test Suite

```sh
$ cargo test
```

| Module | Tests | Names |
|--------|-------|-------|
| `shell::lexer` | 16 | `test_simple_command`, `test_pipes`, `test_redirects`, `test_quotes`, `test_heredoc`, `test_here_string`, `test_glob`, `test_comment`, `test_and_or`, `test_subshell`, `test_less_amp`, `test_empty`, `test_brace_expansion`, `test_clobber_redirect`, `test_dollar_special_vars`, `test_amp_greater_greater` |
| `shell::parser` | 18 | `test_simple_cmd`, `test_pipe`, `test_redirect`, `test_background`, `test_if`, `test_if_elif_else`, `test_for`, `test_and`, `test_or`, `test_assignment`, `test_function`, `test_while`, `test_until`, `test_case`, `test_subshell`, `test_semicolon_compound`, `test_pipeline_chain`, `test_bang_pipe`, `test_redirect_input`, `test_redirect_append`, `test_redirect_heredoc`, `test_dollar_paren`, `test_dollar_paren_nested`, `test_dollar_brace_var`, `test_double_semi`, `test_double_redirect`, `test_hash_after_space_is_comment`, `test_hash_at_start_is_comment`, `test_hash_mid_word`, `test_escape_in_word` |
| `terminal::color` | 10 | `test_hex_to_ansi`, `test_hex_to_ansi_256`, `test_hex_to_ansi_16`, `test_hex_to_ansi_nocolor`, `test_strip_ansi`, `test_strip_ansi_cursor`, `test_strip_ansi_erase`, `test_visible_len`, `test_detect_color_capability` |
| `terminal::prompt` | 3 | `test_shorten_cwd_home`, `test_shorten_cwd_subdir`, `test_shorten_cwd_max_depth` |
| `shell::expand` | 8 | `test_glob_match_simple_star`, `test_glob_match_simple_question`, `test_glob_match_simple_mixed`, `test_glob_match_simple_star_empty_pattern`, `test_glob_match_simple_question_empty`, `test_resolve_user_home`, `test_split_shell_args_simple`, `test_split_shell_args_quoted`, `test_split_shell_args_single_quoted`, `test_split_shell_args_empty`, `test_split_shell_args_mixed` |
| `shell::builtin` | 5 | `test_realpath_no_args`, `test_realpath_current_dir`, `test_realpath_nonexistent`, `test_getopts_advances_past_double_dash` |

**Total: 85 tests, all passing. Zero clippy warnings.**

---

</details>

<details>

<summary>License</summary>

## License

The Unlicense — See [[**`LICENSE`**]](https://github.com/Mapuse/Context/src/branch/shell/LICENSE) for more details.

---

</details>
