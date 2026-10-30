# Copyright 2026 Maxence Faldor
import asyncio
from collections.abc import Mapping
from pathlib import Path

import pytest
from mcp import Client
from mcp.types import CallToolResult, ImageContent, TextContent, Tool

from origamy_mcp import create_server, main

SCRIPT = """\
from origamy import Line, Paper


def fold():
    print("chatter on stdout")
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    paper.valley(Line.through((2, 0), (2, 1)), flap=("1/4", "1/2"))
    return paper
"""


def call(name: str, arguments: Mapping[str, object]) -> CallToolResult:
    async def go() -> CallToolResult:
        async with Client(create_server()) as client:
            return await client.call_tool(name, dict(arguments))

    return asyncio.run(go())


def tools() -> dict[str, Tool]:
    async def go() -> list[Tool]:
        async with Client(create_server()) as client:
            return (await client.list_tools()).tools

    return {tool.name: tool for tool in asyncio.run(go())}


def text_of(result: CallToolResult) -> str:
    content = result.content[0]
    assert isinstance(content, TextContent)
    return content.text


def test_tools_mirror_the_service_requests() -> None:
    found = tools()
    assert set(found) == {"fold_model", "explain_rule"}
    schema = found["fold_model"].input_schema
    assert schema["required"] == ["script"]
    assert "fold()" in schema["properties"]["script"]["description"]


def test_fold_model_reports_the_refusal_and_shows_the_paper(tmp_path: Path) -> None:
    script = tmp_path / "broken.py"
    script.write_text(SCRIPT, encoding="utf-8")
    result = call("fold_model", {"script": str(script)})
    assert "error[no-hinge] step 2" in text_of(result)
    assert sum(isinstance(c, ImageContent) for c in result.content) == 2
    assert result.structured_content is not None
    assert result.structured_content["refused"]["step"] == 2


def test_explain_rule_gives_the_condition() -> None:
    text = text_of(call("explain_rule", {"rule": "layer-crossing"}))
    assert text.startswith("layer-crossing (error, refuses the action)")


def test_a_missing_script_is_a_tool_error(tmp_path: Path) -> None:
    result = call("fold_model", {"script": str(tmp_path / "none.py")})
    assert result.is_error
    assert "no fold script" in text_of(result)


def test_version(capsys: pytest.CaptureFixture[str]) -> None:
    main(["--version"])
    assert capsys.readouterr().out.startswith("origamy-mcp ")
