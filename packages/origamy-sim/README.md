# origamy-sim

A physical simulator of paper sheets, built for origami.

The simulator models how paper bends, creases, stacks, slides and holds, under hands, tools, gravity and the table.
Forces act on the paper at every instant, so a fold is performed by fingers and tools rather than prescribed.
The folding compiler [origamy](https://maxencefaldor.github.io/origamy/) builds on it to fold every model physically.

The [design](https://maxencefaldor.github.io/origamy/simulator/) derives the model from the physics of paper and plans its construction.
Five guarantees hold at every instant of every simulation:

- Paper never passes through paper, a finger, a tool or the table.
- Sheets stay one real paper thickness apart, so stacks and folds take their true size.
- No sheet stretches past a set strain; paper bends instead.
- Each time step reaches its stated tolerance, or reports that it did not.
- The same inputs give the same result, bit for bit, whatever the number of threads.

Every physical and numerical quantity is an input, in SI units, with a documented default.

## Status

Phases 0 and 1 of the plan are done: the foundations, and the elastic sheet.
A sheet stretches, bends and folds along sharp creases, under gravity and held vertices, in implicit or quasi-static time steps.
Benchmarks hold it to known solutions:

- A cantilever bent by its own weight to 30% of its length matches the elastica within 0.1%.
- A Miura-ori folds rigidly to 120° as its creases are actuated, with strain under 10⁻⁸.
- A grid rolled round a cylinder through its vertices takes the cylinder's exact curvature.
- Every gradient and Hessian matches finite differences.

Contact with real thickness comes next, in phase 2.

## Install

Python 3.14 or newer and a Rust toolchain build origamy-sim from source.

```sh
uv add git+https://github.com/maxencefaldor/origamy#subdirectory=packages/origamy-sim
```

## Use

A `Mesh` gives a sheet its flat shape, in meters, as counterclockwise triangles.
Its dihedrals are the interior edges with the vertices opposite them, where the sheet bends.
A `Sheet` adds the paper's material, kami unless given, and its creases.
A `Simulation` steps sheets through time; an infinite time step finds the rest the forces lead to.

```pycon
>>> from origamy_sim import Crease, Mesh, Settings, Sheet, Simulation
>>> side = 0.15  # a 15 cm square of kami
>>> mesh = Mesh([[0, 0], [side, 0], [side, side], [0, side]], [[0, 1, 2], [0, 2, 3]])
>>> mesh.dihedrals  # the diagonal, from vertex 0 to 2, with vertex 3 on its left and 1 on its right
array([[0, 2, 3, 1]])
>>> sheet = Sheet(mesh)
>>> sheet.set_crease((0, 2), Crease(stiffness=0.01, rest_angle=1.0))
>>> simulation = Simulation([sheet], Settings(gravity=(0, 0, 0), time_step=float("inf")))
>>> for vertex in (0, 1, 2):
...     simulation.hold(0, vertex)
>>> simulation.step().converged
True
>>> print(f"{simulation.fold_angles(0)[0]:.6f} rad")
1.000000 rad

```
