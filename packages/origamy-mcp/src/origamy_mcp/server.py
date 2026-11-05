# Copyright 2026 Maxence Faldor
"""MCP server exposing origamy's commands to any agent harness.

The server makes every tool from a request model in `origamy.service`, so tool
arguments, defaults and descriptions match the ``origamy`` command line exactly. Each
tool answers with the text the command line prints, the result as structured content,
and, for builds, the pictures themselves, so the agent sees the paper without another
call. The service runs each fold script in a process of its own, outside the server, so
nothing a script prints reaches the protocol stream.
"""

import base64
import copy
import inspect
import sys
from collections.abc import Awaitable, Callable, Sequence
from importlib.metadata import version
from pathlib import Path
from typing import Annotated, Final, Protocol, runtime_checkable

from mcp.server.mcpserver import MCPServer
from mcp.server.mcpserver.exceptions import ToolError
from mcp.types import (
    CallToolResult,
    ContentBlock,
    ImageContent,
    TextContent,
    ToolAnnotations,
)
from pydantic import BaseModel

from origamy import service
from origamy.errors import OrigamyError

INSTRUCTIONS: Final = """\
origamy folds origami models written as Python fold scripts, exactly, step by step.

Workflow: write a script that defines fold() and returns an origamy.Paper, then call
fold_model on it. Each action in the script either happens or is refused; a refusal
stops the build and names its rule, the faces involved and the script line. The result
holds the report, a picture of the paper at the end (or just before the refused action,
with its fold line and faces marked) as a crease pattern, the folded paper and an x-ray
of its layers, and a picture of every step with the next fold line dashed. Read
everything, change the script, and fold again.

Coordinates are where the paper lies now, seen from above: x to the right, y up.
Unfolded, the paper covers (0, 0) to (width, height), its front up. Numbers are exact:
an int, a Fraction, or text such as '1/2' or '√2 - 1'; floats are refused. Lines come
from origamy.Line.through(p, q) and the constructions fold_onto, bisect, perpendicular,
meet and midpoint; a line no point or line of the paper locates draws a warning.

Actions: valley and mountain fold the paper at a flap point over a line, with the top
`layers` layers there, or all the paper on that side; paper joined to it moves along.
unfold reverses a fold, precrease folds and unfolds, turn_over flips the paper. collapse
sets creases already in the paper to valley, mountain or flat at once, named as lines
or segments of the crease pattern, with valley and mountain seen from the front.
reverse, petal and squash fold compute their creases and collapse. shape, flex and tilt
turn folded paper out of the plane: shape opens a fold about its hinge by an angle,
flex folds creases that meet at a vertex together, and tilt holds the whole model up.
Queries: where maps a point of the unfolded paper to where it lies now, origin maps
back, creases lists the crease pattern segments along a line, layers counts the layers
at a point, and thickest finds the most layers at one point.

view_model draws one step in 3D from any direction, given as an azimuth round the
paper and an elevation above the table; call it once per direction to see a model from
all sides. explain_rule gives each rule's exact condition.
"""

READ_ONLY: Final = ToolAnnotations(read_only_hint=True, open_world_hint=False)
RUNS_CODE: Final = ToolAnnotations(
    read_only_hint=False,
    destructive_hint=True,
    idempotent_hint=False,
    open_world_hint=True,
)
"""A build runs any Python with the user's permissions, so harnesses should ask."""


@runtime_checkable
class Texted(Protocol):
    """A result that writes itself as text for people and agents."""

    @property
    def text(self) -> str:
        """The result as text."""
        ...


@runtime_checkable
class Filed(Protocol):
    """A result that lists the files it wrote."""

    @property
    def files(self) -> tuple[str, ...]:
        """The paths written."""
        ...


def _content(result: BaseModel) -> list[ContentBlock]:
    text = result.text if isinstance(result, Texted) else result.model_dump_json()
    content: list[ContentBlock] = [TextContent(type="text", text=text)]
    if isinstance(result, Filed):
        content.extend(
            ImageContent(
                type="image",
                data=base64.b64encode(Path(path).read_bytes()).decode("ascii"),
                mime_type="image/png",
            )
            for path in result.files
            if path.endswith(".png")
        )
    return content


def tool_from[Request: BaseModel, Result: BaseModel](
    name: str,
    request_type: type[Request],
    handler: Callable[[Request], Awaitable[Result]],
    result_type: type[Result],
) -> Callable[..., Awaitable[CallToolResult]]:
    """An async tool whose parameters are exactly the request model's fields.

    Each parameter's default is a copy of the request field's ``FieldInfo``, so the
    tool schema carries the request model's descriptions, defaults and constraints.
    """
    parameters = [
        inspect.Parameter(
            field,
            inspect.Parameter.KEYWORD_ONLY,
            default=copy.copy(info),
            annotation=info.annotation,
        )
        for field, info in request_type.model_fields.items()
    ]

    async def tool(**arguments: object) -> CallToolResult:
        try:
            result = await handler(request_type.model_validate(arguments))
        except OrigamyError as error:
            raise ToolError(str(error)) from error
        return CallToolResult(
            content=_content(result),
            structured_content=result.model_dump(mode="json"),
        )

    tool.__signature__ = inspect.Signature(  # ty: ignore[unresolved-attribute]
        parameters, return_annotation=Annotated[CallToolResult, result_type]
    )
    tool.__name__ = name
    tool.__doc__ = request_type.__doc__
    return tool


def create_server() -> MCPServer:
    """The origamy MCP server, with every tool."""
    server = MCPServer(
        "origamy",
        version=version("origamy-mcp"),
        instructions=INSTRUCTIONS,
        website_url="https://github.com/maxencefaldor/origamy",
    )
    server.add_tool(
        tool_from(
            "fold_model", service.BuildRequest, service.build, service.BuildReport
        ),
        annotations=RUNS_CODE,
        structured_output=True,
    )
    server.add_tool(
        tool_from("view_model", service.ViewRequest, service.view, service.ViewReport),
        annotations=RUNS_CODE,
        structured_output=True,
    )
    server.add_tool(
        tool_from(
            "explain_rule",
            service.ExplainRequest,
            service.explain,
            service.ExplainResult,
        ),
        annotations=READ_ONLY,
        structured_output=True,
    )
    return server


USAGE: Final = """\
usage: origamy-mcp [--help | --version]

Serve origamy's tools to an agent harness over the Model Context Protocol, on stdio.
"""


def main(argv: Sequence[str] | None = None) -> None:
    """Serve over stdio, or answer ``--help`` or ``--version``."""
    arguments = sys.argv[1:] if argv is None else list(argv)
    if arguments == ["--version"]:
        sys.stdout.write(f"origamy-mcp {version('origamy-mcp')}\n")
        return
    if arguments in (["--help"], ["-h"]):
        sys.stdout.write(USAGE)
        return
    if arguments:
        sys.stderr.write(USAGE)
        raise SystemExit(2)
    if sys.stdin.isatty():
        sys.stderr.write(
            "origamy-mcp serves MCP on stdio for a harness to start; Ctrl-C stops it.\n"
        )
    create_server().run()
