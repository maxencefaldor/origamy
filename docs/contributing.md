# Contributing

The page gives the checks a change must pass, the commands that build the docs, and the conventions for changing origamy.

The rules live in [`AGENTS.md`](https://github.com/maxencefaldor/origamy/blob/main/AGENTS.md) at the repository root, so people and every agent harness read the same rules.
A change is done when five checks pass:

```sh
uv sync
uv run playwright install chromium
uv run ruff format && uv run ruff check && uv run ty check && uv run rumdl check . && uv run pytest
```

The variable `ORIGAMY_RENDER_BROWSER` names a Chromium already on the machine, which then draws instead of Playwright's.
The docs build with [Zensical](https://zensical.org), and their reference pages come from the code:

```sh
uv sync --group docs                # Zensical and mkdocstrings, beside the tools
uv run python docs/reference.py     # write the reference pages generated from the code
uv run python docs/gallery.py       # write a 3D viewer for every example in the gallery
uv run zensical serve               # preview the site at http://localhost:8000
```

## Conventions

- **Feedback is facts:** origamy states what is true about an action and never says how to fix it.
  A refusal names its rule, the faces involved and the script line; advice belongs in a skill.
- **No legal moves:** origamy never lists the actions that would work.
  The agent proposes any action, and origamy says what happens.
- **Mechanisms, not designs:** the API offers exact geometry and folding actions, never a design generator.
- **Exactness:** every coordinate is a `Number`, and every decision is exact.
  Floats appear only in what people read: pictures, the viewer, FOLD files and the angles a message states.
- **Rules, as in ty:** a rule's name is lowercase words joined by hyphens, and its level is `error`, `warn` or `ignore`.
  A rule that makes an action impossible refuses it and is always an error.
- **Fold scripts run in their own process**, never in the command line.
- **Pictures are first-class feedback:** every build draws the crease pattern, the folded paper, an x-ray, and every step.
- **Tests:** every rule gets a case that states it, geometry and folding get property tests, and pictures get a test that checks real pixels.
- **Writing:** each concept has one name, used alike in code, docs, messages and pictures.
  Sentences hold one idea each, under 25 words, in the active voice and the present tense.
