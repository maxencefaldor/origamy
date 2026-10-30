# origamy-mcp

The MCP server for [origamy](https://github.com/maxencefaldor/origamy), a compiler for origami, built for AI agents.

The server serves origamy to any agent harness over the Model Context Protocol, on stdio.
From a clone of the repository, Claude Code adds it with one command:

```sh
claude mcp add origamy -- uv run --directory /path/to/origamy origamy-mcp
```

## Tools

| Tool | Does |
| --- | --- |
| `fold_model` | Runs a fold script in its own process, reports its steps, and returns its pictures |
| `explain_rule` | Gives a rule's level and exact condition, or every rule's |
