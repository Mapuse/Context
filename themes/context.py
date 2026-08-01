# Context default theme
# Cyan accent, git-aware, multi-line capable.

ACCENT = "#22d3ee"
CWD_COLOR = "#a78bfa"
DIM = "#6b7280"
SUCCESS = "#22c55e"
ERROR = "#ef4444"
GIT_COLOR = "#f59e0b"


def render_prompt(**context):
    cwd = context.get("cwd", "~")
    user = context.get("user", "")
    host = context.get("host", "")
    exit_code = int(context.get("exit_code", "0") or "0")
    git = context.get("git_branch", "")
    status = "\u2714" if exit_code == 0 else "\u2718"

    lines_above = []
    if user and host:
        lines_above.append(f"{user}@{host}")
    if git:
        lines_above.append(f" {git}")

    return {
        "lines_above": lines_above,
        "input_prefix": f"{cwd} \u276f ",
        "right_prompt": status,
        "colors": {
            "accent": ACCENT,
            "cwd": CWD_COLOR,
            "success": SUCCESS,
            "error": ERROR,
            "dim": DIM,
        },
    }


def render_right_prompt(**context):
    git = context.get("git_branch", "")
    if not git:
        return ""
    return f" {git}"