# Connect an agent

The page shows how an agent reaches origamy through each of its three front ends.

The command line and the MCP server derive from the same request models, so their arguments always match.

| Front end | Suits |
| --- | --- |
| [Command line](#command-line) | Any harness that runs shell commands, such as Claude Code or Codex |
| [MCP server](#mcp-server) | Any harness that speaks the Model Context Protocol |
| [Skill](#skill) | Claude Code, and any harness that loads instructions from a file |

## Command line

An agent whose harness runs shell commands needs nothing more.
The command `origamy build` runs a fold script and prints the report, and the agent opens the pictures it names.
The option `--output-format json` prints the report as JSON for programs.
The [command line reference](../reference/cli.md) lists every command and option.

```sh
uv run origamy build examples/crane.py
```

## MCP server

The package `origamy-mcp` serves origamy over the [Model Context Protocol](https://modelcontextprotocol.io), on stdio.
Claude Code adds the server from a clone of the repository with one command:

```sh
claude mcp add origamy -- uv run --directory /path/to/origamy origamy-mcp
```

The path `/path/to/origamy` stands for the clone's path.
Any other harness starts the same command from a project's `.mcp.json`:

```json title=".mcp.json"
{
  "mcpServers": {
    "origamy": {
      "command": "uv",
      "args": ["run", "--directory", "/path/to/origamy", "origamy-mcp"]
    }
  }
}
```

The server draws with the Chromium that Playwright installs once per machine (see [Install](../get-started.md#install)).
Each tool matches one command and takes the same arguments:

| Tool | Command | Does |
| --- | --- | --- |
| `fold_model` | `origamy build` | Runs a fold script in its own process, reports its steps, and returns its pictures |
| `explain_rule` | `origamy explain` | Gives a rule's level and exact condition, or every rule's |

Each tool answers twice: with the text the command line prints, for the agent, and with the result as structured content, for programs.
The tool `fold_model` also returns both pictures, so the agent sees the paper without another call.

Two behaviors of the server matter to an agent:

- **Scripts run apart.**
  Each fold script runs in a child process with a time limit, and nothing it prints reaches the protocol stream.
  A fold script still runs as ordinary Python with your permissions, so run only scripts you trust.
  The tool `fold_model` declares that it runs code, so a harness can ask before each call.
- **Self-contained instructions.**
  The server's instructions teach the workflow, the coordinates, the actions and the queries, so an agent can start without the skill.

## Skill

The skill `.claude/skills/fold-model/SKILL.md` teaches an agent the loop: write a fold script, build it, read the report and the pictures, and change the script.
The skill also teaches the coordinates and the actions, and holds no designs.
Claude Code loads the skill on its own inside the origamy repository.
Another project uses the skill once a copy sits in its own `.claude/skills/fold-model/`:

```sh
mkdir -p .claude/skills/fold-model && cp /path/to/origamy/.claude/skills/fold-model/SKILL.md .claude/skills/fold-model/
```

A copy in `~/.claude/skills/fold-model/` serves every project.
Other harnesses can include the file in the agent's context.
The skill in full:

````markdown
--8<-- ".claude/skills/fold-model/SKILL.md"
````
