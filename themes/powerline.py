# Powerline Context theme
# Powerline-style prompt with git branch and exit status.

ACCENT = "#3b82f6"
CWD_COLOR = "#a78bfa"
DIM = "#6b7280"
SUCCESS = "#22c55e"
ERROR = "#ef4444"
GIT_COLOR = "#f59e0b"


def render_prompt(**context):
    cwd = context.get("cwd", "~")
    exit_code = int(context.get("exit_code", "0") or "0")
    git = context.get("git_branch", "")
    user = context.get("user", "")
    host = context.get("host", "")

    status_char = "\u2714" if exit_code == 0 else "\u2718"
    left = f"\ue0b1 {cwd}"
    if git:
        left += f" \ue0b1 {git}"

    lines_above = [f"\ue0b0 {user}@{host}" if user and host else ""]
    lines_above = [l for l in lines_above if l]

    return {
        "lines_above": lines_above,
        "input_prefix": f"{left} \ue0b1 ",
        "right_prompt": status_char,
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
    exit_code = int(context.get("exit_code", "0") or "0")
    status = "\u2714" if exit_code == 0 else "\u2718"
    out = status
    if git:
        out = f"{git} {out}"
    return out