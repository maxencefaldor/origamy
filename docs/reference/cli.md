# Command line

The `origamy` command builds fold scripts and explains rules from the shell.

The command `origamy --version` prints the version.
The command `origamy build` exits with status 1 when an action is refused or the script fails, and 0 otherwise.
Both commands exit with status 2 on an unknown option or a missing argument, and on errors such as a missing script.
Each command's options come from a request model in `origamy.service`, which the [MCP server](../agents/connect.md#mcp-server) shares, so a tool takes the same arguments as its command.
The sections below hold each command's help text, as `--help` prints it.

## `origamy`

The command names its two subcommands.

```text
usage: origamy [-h] {build,explain} ...

Fold origami models written as Python fold scripts.

options:
  -h, --help       show this help message and exit

subcommands:
  {build,explain}
    build          Run a fold script, then report, draw and export the paper.
    explain        Explain rules and their levels.
```

## `origamy build`

Run a fold script, report its steps, draw them and write a FOLD file.

```text
usage: origamy build [-h] [--output-format {text,json}] [--pictures | --no-pictures]
                     [--timeout float]
                     SCRIPT

Run a fold script, then report its steps and draw and export the paper.

positional arguments:
  SCRIPT                The fold script: a Python file that defines fold(), returning
                        a Paper.

options:
  -h, --help            show this help message and exit
  --output-format {text,json}
                        'text' for people and agents, 'json' for programs. (default:
                        text)
  --pictures, --no-pictures
                        Draw the build's pictures. (default: True)
  --timeout float       Seconds the script may run before it stops. (default: 300)
```

## `origamy explain`

Print every rule, or one rule, with its level and exact condition.

```text
usage: origamy explain [-h] [RULE]

Explain the rules: each one's level and exact condition.

positional arguments:
  RULE        The rule to explain; every rule when omitted. (default: None)

options:
  -h, --help  show this help message and exit
```
