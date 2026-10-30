# Copyright 2026 Maxence Faldor
"""Write the reference pages that come from the code: the command line and the rules.

Run from the repository root: ``uv run python docs/reference.py``. The command line page
holds the help text that ``origamy --help`` and each subcommand's ``--help`` print, and
the rules page holds every rule of :data:`origamy.rules.RULES`, as ``origamy explain``
prints them. The pages therefore match the tools exactly.
"""

import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Final

from origamy.rules import RULES

REFERENCE: Final = Path(__file__).resolve().parent / "reference"

COLUMNS: Final = "88"
"""The width the help text wraps at, the same on every machine."""

CLI_INTRODUCTION: Final = """\
# Command line

The `origamy` command builds fold scripts and explains rules from the shell.

The command `origamy --version` prints the version.
The command `origamy build` exits with status 1 when an action is refused or the \
script fails, and 0 otherwise.
Both commands exit with status 2 on an unknown option or a missing argument, and on \
errors such as a missing script.
Each command's options come from a request model in `origamy.service`, which the \
[MCP server](../agents/connect.md#mcp-server) shares, so a tool takes the same \
arguments as its command.
The sections below hold each command's help text, as `--help` prints it.
"""

COMMANDS: Final = (
    ((), "The command names its two subcommands."),
    (
        ("build",),
        "Run a fold script, report its steps, draw them and write a FOLD file.",
    ),
    (
        ("explain",),
        "Print every rule, or one rule, with its level and exact condition.",
    ),
)
"""Each command's arguments, after ``origamy``, and what it does."""


def help_text(arguments: tuple[str, ...]) -> str:
    """Return what ``origamy ARGUMENTS --help`` prints, at a fixed width, uncolored."""
    environment = {**os.environ, "COLUMNS": COLUMNS, "PYTHON_COLORS": "0"}
    completed = subprocess.run(  # noqa: S603 - runs this Python with fixed arguments
        [
            sys.executable,
            "-c",
            "from origamy.cli import main; main()",
            *arguments,
            "--help",
        ],
        capture_output=True,
        check=True,
        env=environment,
        text=True,
    )
    return completed.stdout.rstrip()


def cli_page() -> str:
    """Return the command line page: each command's purpose, then its help text."""
    lines = [CLI_INTRODUCTION]
    for arguments, summary in COMMANDS:
        name = " ".join(("origamy", *arguments))
        lines.extend(
            (
                f"## `{name}`",
                "",
                summary,
                "",
                "```text",
                help_text(arguments),
                "```",
                "",
            )
        )
    return "\n".join(lines).rstrip() + "\n"


RULES_INTRODUCTION: Final = """\
# Rules

Each rule names one fact a build can state about a step, and `origamy explain` prints \
the same condition as this page.
A refusal rule stops the action it states: the paper has no state after it, so the \
build stops at that script line.
A refusal rule is always an error, and the other rules state a fact about a step the \
paper takes (see [Feedback](../guide/feedback.md)).
"""

_SENTENCE_END: Final = re.compile(r"(?<=[.:])\s+(?=[A-Z`(])")


def rules_page() -> str:
    """Return the page of rules: a table, then each rule's summary and condition."""
    lines = [
        RULES_INTRODUCTION,
        "| Rule | Level | Refuses the action | Fact |",
        "| --- | --- | --- | --- |",
        *(
            f"| [`{name}`](#{name}) | `{rule.level}` | "
            f"{'yes' if rule.refusal else 'no'} | {rule.summary} |"
            for name, rule in RULES.items()
        ),
    ]
    for name, rule in RULES.items():
        lines.extend(("", f"## {name}", "", rule.summary, ""))
        lines.extend(_SENTENCE_END.split(rule.condition))
    return "\n".join(lines) + "\n"


def main() -> None:
    """Write every page."""
    REFERENCE.mkdir(parents=True, exist_ok=True)
    pages = {
        REFERENCE / "cli.md": cli_page(),
        REFERENCE / "rules.md": rules_page(),
    }
    for page, text in pages.items():
        page.write_text(text, encoding="utf-8")
        print(f"wrote {page}")


if __name__ == "__main__":
    main()
