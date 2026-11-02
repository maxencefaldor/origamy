# Working on origamy

The repository holds origamy, a compiler for origami, built for AI agents, and origamy-sim, the physical simulator of paper it builds on.
The README says what origamy does; the rules below say how to change it.

## Commands

```sh
uv sync                             # install Python 3.14 and every tool from uv.lock, and build the simulator's core
uv run playwright install chromium  # the browser origamy draws in, once per machine
uv run pytest                       # tests; warnings are errors
uv run ruff format                  # format
uv run ruff check                   # lint, with every rule enabled
uv run ty check                     # types
uv run rumdl check .                # Markdown
uv run python docs/reference.py     # the generated reference pages
uv run python docs/gallery.py       # the gallery's viewers; --thumbnails redraws its cards
uv run python docs/simulator.py     # the simulator's runs the docs show
uv run zensical serve               # the docs site, at http://localhost:8000
uv run python packages/origamy-sim/codegen/generate.py  # the simulator's generated derivatives
cargo fmt --all                     # format Rust
cargo test                          # Rust tests of the simulator's core
cargo bench                         # time steps of the simulator's benchmark scenes
PYO3_PYTHON=$PWD/.venv/bin/python cargo clippy --workspace --all-targets -- -D warnings  # lint Rust
```

A change is done when every check above passes.
Plain cargo commands build the simulator's core alone; its bindings compile against Python 3.14, which `PYO3_PYTHON` names.
To draw with a Chromium you already have, set `ORIGAMY_RENDER_BROWSER` to its executable instead of installing Playwright's.

## Architecture

The package, `packages/origamy/src/origamy`, is ordered.
Each module imports only from the modules listed before it:

1. `errors`: the errors front ends show as messages.
1. `field`, `geometry`, `constructions`: exact numbers in a tower of square roots, points, lines, isometries and convex polygons, and the Huzita-Justin constructions.
1. `state`, `space`: the crease pattern, each face's place, and the stacking; and the form shaping gives paper in space.
1. `layers`: the layer solver's constraints, propagation and search.
1. `rules`, `checks`: every fact a build states, and the checks that warn.
1. `actions`: fold, unfold, precrease, collapse and turn over, each a function from one state to the next.
1. `moves`, `shape`, `flex`: named moves, such as reverse and petal folds, which compute their creases and collapse; shape steps and tilts; and rigid flexes.
1. `provenance`, `paper`: the script lines behind actions, and the `Paper` a fold script folds.
1. `result`, `runner`: a build as exact data, and fold scripts run in their own process.
1. `draw`, `render`, `exports`, `viewer`: SVG views, PNG pictures through Chromium, FOLD files, and the standalone 3D viewer.
1. `service`, `cli`: every command defined once, and the command line that adapts it.

The MCP server, `origamy_mcp` in `packages/origamy-mcp`, makes a tool of each service command, with the same arguments.

The simulator, `packages/origamy-sim`, is a Rust core with Python bindings, designed in `docs/simulator/design.md`:

| Path | Holds |
| --- | --- |
| `crates/origamy-sim` | The core, with no Python: meshes, materials, energies, contact and the Newton solver |
| `crates/origamy-sim-py` | The bindings, which make the core the module `origamy_sim._core` |
| `src/origamy_sim` | The Python package: the core's classes, typed by `_core.pyi`, and the pydantic models of every parameter |
| `codegen` | The SymPy expressions behind the generated derivative code |

The simulator knows nothing of origami: its interface speaks of meshes, materials, bodies and forces.

## Conventions

- **Tools:** the standard, official tool in each category: uv, ruff, ty, rumdl, pytest with Hypothesis, pydantic and pydantic-settings, and Playwright.
  Add a dependency only when it is high-quality and used well.
  The exact kernel uses the standard library alone, and numpy serves the numeric solvers that shape paper in space.
- **Format and lint:** [maxencefaldor/toolchain](https://github.com/maxencefaldor/toolchain): ruff's default format, every ruff rule, every ty rule as an error, and Markdown one sentence per line.
  A rule is switched off only in `pyproject.toml`, as narrowly as it can be, with its reason beside it.
- **Python:** 3.14 or newer, with modern syntax: PEP 695 generics and `type` aliases.
- **Concurrency:** asyncio only.
- **Exactness:** every coordinate is a `Number`, and every decision is exact.
  A float estimate carries an error bound and decides a sign only where the bound leaves it clear.
  Floats otherwise appear only in what people read: pictures, the viewer, FOLD files and the angles a message states.
  The form is the one exception: shaping turns paper by any angle, so a form is floats, and its checks compare motions within a tolerance.
  A square root that no level of the tower holds adds a level, so the first five Huzita-Justin axioms stay exact.
  A build carries its tower in its data.
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
- **The viewer** is `assets/viewer.js` and `assets/viewer.css`, written into one HTML page with the build as data.
  The viewer draws paper as paper: a fold's crease bends round between its layers, and a collapse turns its creases together.
  The viewer loads three.js from jsDelivr; `assets/three` vendors the same version, unmodified, so tests run offline.
- **Rust:** `rust-toolchain.toml` pins the toolchain, and `Cargo.lock` every crate.
  The code is formatted by rustfmt and linted by clippy's pedantic group as errors, and the core forbids unsafe code.
  PyO3 and maturin build the bindings, and proptest tests properties.
- **Physics:** SI units and 64-bit floats.
  Every physical and numerical quantity is an input with a documented default, and a preset, such as kami, is a named set of values.
- **Parameters:** in Python, every parameter group is a frozen pydantic model with field constraints and model validators, and the bindings take those models.
  The core checks every value again, and a property test holds the two checks to one verdict.
  Configuration reads them through pydantic-settings, from YAML and the command line.
- **Derivatives:** every energy has an exact gradient and Hessian.
  Hard ones are generated from SymPy expressions by `codegen/generate.py`, and CI checks the generated code is current.
- **Determinism:** derived tables come out sorted, never in hash order, and parallel work sums in a fixed order.
- **Bindings stay thin:** they convert arrays and parameter models, call the core and convert back.
  Their doc comments are the Python docstrings, in Google style.
  A core error is a typed value naming the element at fault, and Python sees it as a `ValueError` with the same message.
  `_core.pyi` types every class of `_core`, and a test holds the two together.
- **Indices:** 32-bit in the core, and 64-bit integer arrays in Python, so arithmetic on them never wraps.
- **Tests:** every rule gets a case that states it.
  Geometry and folding get property tests.
  Pictures get a test that checks real pixels.
  Every energy of the simulator gets finite-difference checks of its gradient and Hessian.

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
| shape step | An action that turns folded paper out of the plane about a hinge, by an angle |
| tilt | An action that turns the whole paper in space about a line on the table |
| flex | An action that folds creases to angles while the other creases follow as rigid origami |
| form | Where each face lies in space once shaping turns it: a rigid motion of its place |
| named move | A collapse that computes its own creases, such as a reverse or petal fold |
| action, step | What a script asks the paper to do, and an action the paper took |
| refusal | An action the paper cannot take, with the facts of why |
| fold script | A Python file whose `fold()` returns a `Paper`; "script" for short |
| build | One run of a fold script: its steps reported, drawn and exported |
| x-ray | A picture of every layer at once, darker where more layers lie |
| sheet | A piece of paper in a simulation |
| mesh | A sheet's rest geometry: its flat shape, as counterclockwise triangles |
| rest position | Where a vertex lies in the flat sheet, in meters |
| dihedral | An interior edge of a mesh with its two triangles, where the sheet bends |
| crease band | The strip along a crease where the paper has yielded, a few tenths of a millimeter wide: parallel lines of edges that share the crease's turn |
| sharp crease | A crease band narrowed onto mesh edges, for coarse meshes |
| springback | The angle a released crease opens by: its yield moment over its stiffness |
| plastic rotation | The rotation a crease has yielded and crept through, which hardening reads |
| aging | The creep of a crease's rest angle toward its fold, faster the larger its moment |
| time step | One interval of simulated time: a minimization of the incremental potential |
| lag | One solve of a time step with friction fixed at a state: the step's start, then each solution it reaches |
| stick distance | The slip within one time step under which friction holds a contact |

Spell American English: color, center.

## Writing

Docs, docstrings, comments and messages are formal, direct and plain.

- **Sentences:** one idea each, under 25 words, subject first, active voice, present tense.
  No sentence starts with a lowercase word or with "Because", "It", "There is", "This" or "Note that".
- **Words:** say what something is or does, not what it is not.
  Lists hold parallel items, tables hold lookups, and prose holds reasoning.
