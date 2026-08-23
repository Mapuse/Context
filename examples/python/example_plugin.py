"""
context unified plugin — one single .py file, everything inside.

This file IS the plugin. Any Python library works: rich, textual,
npyscreen, prompt_toolkit, blessed, urwid, asciimatics, etc.

EVERY public function (not starting with _) is automatically discovered
as a hook. Define any functions you want — context finds and calls them.

Hook functions receive keyword arguments with shell context:
    on_startup()
    on_exit()
    on_preexec(command=...)
    on_postexec(command=..., exit_code=..., duration_ms=..., cwd=...,
                env_USER=..., env_HOME=..., ...)
    on_precmd()
    on_postcmd()
    on_command_not_found(command=...)
    on_prompt(cwd=..., exit_code=..., ...)
    on_keypress(key=...)
    custom_hook(...)

You can define ANY function name — context auto-discovers all public functions.

If you need a venv, set [python] venv_path in c.toml or activate inline:

    import sys, os
    venv = os.path.expanduser("~/.venvs/myplugin")
    site = os.path.join(venv, "lib", f"python{sys.version_info.major}.{sys.version_info.minor}", "site-packages")
    if os.path.isdir(site):
        sys.path.insert(0, site)
"""

# Uncomment to activate a venv inline:
# import sys, os
# _venv = os.path.expanduser("~/.venvs/myplugin")
# _site = os.path.join(_venv, "lib", f"python{sys.version_info.major}.{sys.version_info.minor}", "site-packages")
# if os.path.isdir(_site):
#     sys.path.insert(0, _site)

# import rich, textual, npyscreen, prompt_toolkit, blessed, urwid

import time

_start_time = time.time()
_command_count = 0
_last_command = ""


def on_startup():
    global _start_time
    _start_time = time.time()


def on_preexec(command):
    pass


def on_postexec(command, exit_code, duration_ms="", cwd="", **env):
    global _command_count, _last_command
    _command_count += 1
    _last_command = command


def on_exit():
    uptime = time.time() - _start_time
    print(f"\n[context] session: {_command_count} commands, {uptime:.0f}s", flush=True)


def on_prompt(cwd, exit_code, **env):
    pass


def on_keypress(key):
    pass


def my_custom_hook(**kwargs):
    """Any function name works. context auto-discovers all public functions."""
    pass


def run():
    """
    TUI Mode — only called if [python] tui_mode = true.
    Takes over the terminal completely. When this returns, context exits.
    Use any library: rich, textual, npyscreen, blessed, urwid, etc.
    """
    print("context plugin TUI mode — define your full TUI here")
    print("Press Enter to exit.")
    try:
        input()
    except (EOFError, KeyboardInterrupt):
        pass
    return True
