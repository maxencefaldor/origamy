# Copyright 2026 Maxence Faldor
"""A Griffe extension that shows the doctests in docstrings as Python sessions.

The docstrings of origamy hold doctests outside any ``Examples:`` section, and Markdown
reads a line that starts with ``>>>`` as a quote. The extension fences each doctest as a
``pycon`` block before mkdocstrings renders the docstring. The file ``zensical.toml``
loads it by path, so the docs build needs no package of its own.
"""

import importlib
import re
from typing import Final, override

griffe = importlib.import_module("griffe")

_PROMPT: Final = re.compile(r"^(\s*)>>> ")
_FENCE: Final = "```"


def fenced(text: str) -> str:
    """Return the text with each doctest, its prompts and its output, in a fence.

    A doctest runs from a line that starts with ``>>>`` to the next blank line, as
    doctest reads it. Lines already inside a fence stay as they are.
    """
    lines = text.splitlines()
    result: list[str] = []
    inside = False
    index = 0
    while index < len(lines):
        line = lines[index]
        prompt = None if inside else _PROMPT.match(line)
        if prompt is None:
            inside ^= line.lstrip().startswith(_FENCE)
            result.append(line)
            index += 1
            continue
        indent = prompt.group(1)
        result.append(f"{indent}{_FENCE}pycon")
        while index < len(lines) and lines[index].strip():
            result.append(lines[index])
            index += 1
        result.append(f"{indent}{_FENCE}")
    return "\n".join(result)


class DoctestFences(griffe.Extension):
    """Fence the doctests of every docstring once the object tree is loaded."""

    @override
    def on_object(self, *, obj: object, **_: object) -> None:
        """Fence the doctests in one object's docstring."""
        if getattr(obj, "is_alias", False):
            return
        docstring = getattr(obj, "docstring", None)
        if docstring is not None and ">>>" in docstring.value:
            docstring.value = fenced(docstring.value)
