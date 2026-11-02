# Copyright 2026 Maxence Faldor
"""A griffe extension that documents origamy-sim's compiled core with its docstrings.

The stub ``_core.pyi`` types the compiled module ``origamy_sim._core``, and the Rust doc
comments are its docstrings, at runtime. Griffe reads the stub, so this extension copies
each docstring the stub lacks from the object the module defines.
"""

import importlib
import inspect
from dataclasses import dataclass
from typing import Final, override

import griffe

PACKAGE: Final = "origamy_sim"
CORE: Final = "origamy_sim._core"


class RuntimeDocstrings(griffe.Extension):
    """Fills the compiled core's docstrings from its runtime objects."""

    @override
    def on_package(
        self, *, pkg: griffe.Module, loader: griffe.GriffeLoader, **kwargs: object
    ) -> None:
        if pkg.path != PACKAGE:
            return
        style = Style(loader.docstring_parser, loader.docstring_options)
        # The core's classes name the package as their module, so griffe finds each one
        # through the package's export of it, typed by the stub.
        core = importlib.import_module(CORE)
        for name in pkg.exports or ():
            runtime = getattr(core, str(name), None)
            if runtime is not None:
                fill(pkg[str(name)].final_target, runtime, style)


@dataclass(frozen=True)
class Style:
    """How the docs parse docstrings: the parser and its options."""

    parser: griffe.DocstringStyle | griffe.Parser | None
    options: griffe.DocstringOptions | None


def fill(obj: griffe.Object | griffe.Alias, runtime: object, style: Style) -> None:
    """Gives an object, and each of its members, the docstring it has at runtime."""
    if isinstance(obj, griffe.Alias):
        return
    text = inspect.getdoc(runtime)
    if obj.docstring is None and text is not None and text != "":
        obj.docstring = griffe.Docstring(
            text, parent=obj, parser=style.parser, parser_options=style.options
        )
    for name, member in obj.members.items():
        inner = inspect.getattr_static(runtime, name, None)
        if inner is not None:
            fill(member, inner, style)
