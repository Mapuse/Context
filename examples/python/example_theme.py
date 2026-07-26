"""
context unified theme — one single .py file, everything inside.

This file IS the theme. Any Python library works: rich, textual,
npyscreen, prompt_toolkit, blessed, urwid, asciimatics, etc.

Everything the shell knows is passed to your functions as keyword args.
Read whatever you need, return whatever you want.

Available in context dict (passed as **kwargs):
    cwd, user, host, exit_code, git_branch, shell_version, shell_name,
    terminal_width, terminal_height, width,
    env_TERM, env_PATH, env_HOME, ... (all env vars prefixed with env_)

Functions context calls (all optional, define any or none):
    render_prompt(**context)       → dict (prompt layout)
    render_right_prompt(**context) → str
    render_command_summary(**context) → str
    run()                     → TUI Mode (needs tui_mode = true)

Colors returned in dict["colors"] can have ANY keys.
context reads: accent, success, error, primary, dim, text, cwd — or any
custom key you define.

If you need a venv, set [python] venv_path in c.toml or activate inline:

    import sys, os
    venv = os.path.expanduser("~/.venvs/mytheme")
    site = os.path.join(venv, "lib", f"python{sys.version_info.major}.{sys.version_info.minor}", "site-packages")
    if os.path.isdir(site):
        sys.path.insert(0, site)
"""

# Uncomment to activate a venv inline:
# import sys, os
# _venv = os.path.expanduser("~/.venvs/mytheme")
# _site = os.path.join(_venv, "lib", f"python{sys.version_info.major}.{sys.version_info.minor}", "site-packages")
# if os.path.isdir(_site):
#     sys.path.insert(0, _site)

# import rich, textual, npyscreen, prompt_toolkit, blessed, urwid


def render_prompt(**context):
    """
    Called every prompt cycle.
    context contains everything: cwd, user, host, exit_code, git_branch,
    shell_version, terminal_width, env_PATH, env_HOME, etc.
    Return a dict with any/all of:
        lines_above: list[str]  — lines above the input
        input_prefix: str       — the prompt before the cursor
        right_prompt: str       — right-side prompt
        colors: dict            — ANY color keys (accent, success, etc.)
        any other keys          — accessible in extra dict
    Or just return a string and context uses it as input_prefix.
    """
    cwd = context.get("cwd", "~")
    exit_code = int(context.get("exit_code", "0"))
    git = context.get("git_branch", "")
    version = context.get("shell_version", "")
    status = "\u2714" if exit_code == 0 else "\u2718"

    lines_above = []
    if git:
        lines_above.append(f" ({git})")

    return {
        "lines_above": lines_above,
        "input_prefix": f"{cwd} \u276f ",
        "right_prompt": f"{status} v{version}",
        "colors": {
            "accent": "#3b82f6",
            "success": "#22c55e",
            "error": "#ef4444",
            "cwd": "#a78bfa",
        },
    }


def render_right_prompt(**context):
    git = context.get("git_branch", "")
    return f"({git})" if git else ""


def render_command_summary(**context):
    command = context.get("command", "")
    exit_code = int(context.get("exit_code", "0"))
    duration = context.get("duration_ms", "0")
    status = "\u2714" if exit_code == 0 else "\u2718"
    return f"{status} {command} ({duration}ms)"


def run():
    """
    TUI Mode — only called if [python] tui_mode = true.
    Takes over the terminal completely. When this returns, context exits.
    Use any library: rich, textual, npyscreen, blessed, urwid, etc.
    """
    print("context TUI mode — define your full TUI here")
    print("Press Enter to exit.")
    try:
        input()
    except (EOFError, KeyboardInterrupt):
        pass
    return True
