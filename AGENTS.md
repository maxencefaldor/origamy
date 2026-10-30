# Working on origamy

The repository holds origamy, a compiler for origami, built for AI agents.
The README says what origamy does; the rules below say how to change it.

## Commands

```sh
uv sync                             # install Python 3.14 and every tool from uv.lock
uv run playwright install chromium  # the browser origamy draws in, once per machine
uv run pytest                       # tests; warnings are errors
uv run ruff format                  # format
uv run ruff check                   # lint, with every rule enabled
uv run ty check                     # types
uv run rumdl check .                # Markdown
```

A change is done when all five checks pass.
To draw with a Chromium you already have, set `ORIGAMY_RENDER_BROWSER` to its executable instead of installing Playwright's.

## Architecture

The package, `packages/origamy/src/origamy`, is ordered.
Each module imports only from the modules listed before it:

1. `errors`: the errors front ends show as messages.
1. `field`, `geometry`, `constructions`: exact numbers a + b√2, points, lines, isometries and convex polygons, and the Huzita-Justin constructions.
1. `state`: the crease pattern, each face's place, and the stacking.
1. `layers`: the layer solver's constraints, propagation and search.
1. `rules`, `checks`: every fact a build states, and the checks that warn.
1. `actions`: fold, unfold, precrease, collapse and turn over, each a function from one state to the next.
1. `moves`: named moves, such as reverse and petal folds, each computing its creases and collapsing.
1. `provenance`, `paper`: the script lines behind actions, and the `Paper` a fold script folds.
1. `result`, `runner`: a build as exact data, and fold scripts run in their own process.
1. `draw`, `render`, `exports`: SVG views, PNG pictures through Chromium, and FOLD files.
1. `service`, `cli`: every command defined once, and the command line that adapts it.

The MCP server, `origamy_mcp` in `packages/origamy-mcp`, makes a tool of each service command, with the same arguments.

## Conventions

- **Tools:** the standard, official tool in each category: uv, ruff, ty, rumdl, pytest with Hypothesis, pydantic and pydantic-settings, and Playwright.
  Add a dependency only when it is high-quality and used well; the kernel uses the standard library alone.
- **Format and lint:** [maxencefaldor/toolchain](https://github.com/maxencefaldor/toolchain): ruff's default format, every ruff rule, every ty rule as an error, and Markdown one sentence per line.
  A rule is switched off only in `pyproject.toml`, as narrowly as it can be, with its reason beside it.
- **Python:** 3.14 or newer, with modern syntax: PEP 695 generics and `type` aliases.
- **Concurrency:** asyncio only.
- **Exactness:** no floats in the kernel.
  Every coordinate is a `Number`; floats appear only in pictures and FOLD files.
- **Store once:** a state stores its vertices, faces with their places, and the stacking.
  Whether a crease is folded, and its mountain or valley assignment, derive from them.
- **Feedback is facts:** origamy states what is true about an action and never says how to fix it.
  A refusal names its rule, the faces involved and the script line; advice belongs in a skill, not in origamy.
- **No legal moves:** origamy never lists the actions that would work.
  The agent proposes any action, and origamy says what happens.
- **Mechanisms, not designs:** the API offers exact geometry and folding actions, never a design generator.
- **Rules, as in ty:** a rule's name is lowercase words joined by hyphens, and its level is `error`, `warn` or `ignore`.
  A rule that makes an action impossible refuses it and is always an error.
- **Fold scripts run in their own process** (`origamy.runner`), never in the command line.
- **Pictures are first-class feedback:** every build draws the crease pattern, the folded paper and an x-ray, and every step.
- **Tests:** every rule gets a case that states it.
  Geometry and folding get property tests.
  Pictures get a test that checks real pixels.

## Vocabulary

Each concept has exactly one name, used alike in code, docs, messages and pictures.

| Term | Meaning |
| --- | --- |
| paper | The sheet a fold script folds, its front up at the start |
| crease pattern | The paper unfolded, with every crease |
| face | A convex region of the paper bounded by creases and the paper's edges |
| place | Where a face lies once folded: an isometry of the plane |
| stacking | Which face is above, wherever two faces overlap |
| layer | One face at a point; the layers at a point are the faces that cover it |
| crease, fold | An edge between two faces, and a crease whose faces lie on one side of it |
| valley, mountain | A fold toward the viewer, and away from the viewer |
| flap point | The point at which an action takes the paper to move |
| hinge | A crease along the fold line between paper that moves and paper that stays |
| collapse | An action that folds, unfolds or reverses many creases at once |
| named move | A collapse that computes its own creases, such as a reverse or petal fold |
| action, step | What a script asks the paper to do, and an action the paper took |
| refusal | An action the paper cannot take, with the facts of why |
| fold script | A Python file whose `fold()` returns a `Paper`; "script" for short |
| build | One run of a fold script: its steps reported, drawn and exported |
| x-ray | A picture of every layer at once, darker where more layers lie |

Spell American English: color, center.

## Writing

Docs, docstrings, comments and messages are formal, direct and plain.

- **Sentences:** one idea each, under 25 words, subject first, active voice, present tense.
  No sentence starts with a lowercase word or with "Because", "It", "There is", "This" or "Note that".
- **Words:** say what something is or does, not what it is not.
  Lists hold parallel items, tables hold lookups, and prose holds reasoning.
