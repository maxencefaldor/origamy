# Copyright 2026 Maxence Faldor
"""The ``origamy`` command line.

Each subcommand is a request model from `origamy.service` plus an output format, so the
command line accepts exactly what the service does.
"""

import asyncio
import sys
from importlib.metadata import version
from typing import Literal, override

from pydantic import BaseModel, Field
from pydantic_settings import (
    BaseSettings,
    CliApp,
    CliSubCommand,
    PydanticBaseSettingsSource,
    SettingsConfigDict,
)

from origamy import service
from origamy.errors import OrigamyError


class _Output(BaseModel):
    output_format: Literal["text", "json"] = Field(
        default="text", description="'text' for people and agents, 'json' for programs."
    )


class BuildCommand(service.BuildRequest, _Output):
    """Run a fold script, then report its steps and draw and export the paper."""

    def cli_cmd(self) -> None:
        """Run the command."""
        report = asyncio.run(service.build(self))
        print(
            report.model_dump_json(indent=2)
            if self.output_format == "json"
            else report.text
        )
        if not report.ok:
            raise SystemExit(1)


class ExplainCommand(service.ExplainRequest):
    """Explain the rules: each one's level and exact condition."""

    def cli_cmd(self) -> None:
        """Run the command."""
        print(service.explain(self))


class Origamy(BaseSettings):
    """Fold origami models written as Python fold scripts."""

    model_config = SettingsConfigDict(
        cli_prog_name="origamy",
        cli_kebab_case=True,
        cli_implicit_flags=True,
        cli_hide_none_type=True,
        cli_avoid_json=True,
        cli_enforce_required=True,
    )

    build: CliSubCommand[BuildCommand] = Field(
        description="Run a fold script, then report, draw and export the paper."
    )
    explain: CliSubCommand[ExplainCommand] = Field(
        description="Explain rules and their levels."
    )

    @classmethod
    @override
    def settings_customise_sources(
        cls,
        settings_cls: type[BaseSettings],
        init_settings: PydanticBaseSettingsSource,
        env_settings: PydanticBaseSettingsSource,
        dotenv_settings: PydanticBaseSettingsSource,
        file_secret_settings: PydanticBaseSettingsSource,
    ) -> tuple[PydanticBaseSettingsSource, ...]:
        """Read the command line only."""
        return (init_settings,)

    def cli_cmd(self) -> None:
        """Run the chosen subcommand."""
        CliApp.run_subcommand(self)


def main() -> None:
    """Run the command line."""
    if sys.argv[1:] in (["--version"], ["-V"]):
        print(f"origamy {version('origamy')}")
        return
    try:
        CliApp.run(Origamy)
    except OrigamyError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2) from None
