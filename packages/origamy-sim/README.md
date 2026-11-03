# origamy-sim

A physical simulator of paper sheets, built for origami.

The simulator models how paper bends, creases, stacks, slides and holds, under hands, tools, gravity and the table.
Forces act on the paper at every instant, so a fold is performed by fingers and tools rather than prescribed.
The folding compiler [origamy](https://maxencefaldor.github.io/origamy/) builds on it to fold every model physically.

The [design](https://maxencefaldor.github.io/origamy/simulator/design/) derives the model from the physics of paper and plans its construction.
Five guarantees hold at every instant of every simulation:

- Paper never passes through paper, a finger, a tool or the table.
- Sheets stay one real paper thickness apart, so stacks and folds take their true size.
- No sheet stretches past a set strain; paper bends instead.
- Each time step reaches its stated tolerance, or reports that it did not.
- The same inputs give the same result, bit for bit, whatever the number of threads.

Every physical and numerical quantity is an input, in SI units, with a documented default.

## Status

Phases 0 to 4 of the plan are done: the foundations, the elastic sheet, contact with real thickness, friction and dynamics, and paper's creases.
A sheet stretches, bends and folds along sharp creases, under gravity and held vertices, in implicit or quasi-static time steps.
Paper never comes closer than its thickness to paper or to the table.
Coulomb friction holds paper on paper and on the table, and air drags paper moving across its surface.
A time step that does not converge retries as two half steps.
Creases yield past their yield moment and spring back when released, as paper's creases do.
Held folds age: they lose moment logarithmically in time, and set deeper the longer they are held.
Hands carry held vertices within each time step, so paper never crosses paper on the way, and each hand reports the force it exerts.
Rigid spheres and capsules, such as fingertips and tools, touch paper with friction of their own.
A fingertip grips paper and carries it, and its pad yields as it presses.
Point forces and pressure load the paper.
Benchmarks hold it to known solutions:

- A cantilever bent by its own weight to 30% of its length matches the elastica within 0.1%.
- A Miura-ori folds rigidly to 120° as its creases are actuated, with strain under 10⁻⁸.
- A grid rolled round a cylinder through its vertices takes the cylinder's exact curvature.
- A strip curved at rest rolls into its rest shape within 1 nm.
- Four sheets dropped on the table stack a thickness apart, each within the barrier's 2 µm gap.
- A strip folded over itself onto the table never brings paper within its thickness of paper.
- A square of kami slides down an incline, on the table or on paper, within 1% of Coulomb's law.
- Below its friction angle, the square creeps by less than the 0.1 µm stick distance a step.
- A sheet falling flat through air follows implicit Euler within 10 nm/s, to its terminal velocity of 0.91 m/s within 0.1%.
- A crease of kami pressed flat springs back to an opening of 38°, within the measured 30° to 40°.
- A fold held ten times longer sets about 5.7° deeper, as paper measures.
- A crease band pressed fully flat springs back to 36°, exactly as its sharp crease would.
- A hand lifting a sheet by its edge carries its weight within 10⁻⁶, and one pulling it across the table feels Coulomb's friction within 1%.
- A fingertip pressing paper on the table drags it within 1% when its friction beats the table's, and slides over it otherwise.
- A strip of kami that a fingertip folds in half and creases opens to 35.4° pressed with 1 N, and 35.3° with 4 N.
- Every gradient and Hessian matches finite differences, element by element and for the whole system.

Phases 0 to 5 are done: hands hold, carry, grip and press paper, and thickening comes next.

## Install

Python 3.14 or newer and a Rust toolchain build origamy-sim from source.

```sh
uv add git+https://github.com/maxencefaldor/origamy#subdirectory=packages/origamy-sim
```

## Use

A `Mesh` gives a sheet its flat shape, in meters, as counterclockwise triangles.
Its dihedrals are the interior edges with the vertices opposite them, where the sheet bends.
A `Sheet` adds the paper's material, kami unless given, its sharp creases, and the angle each dihedral rests at.
A `Simulation` steps sheets through time; an infinite time step finds the rest the forces lead to.

```pycon
>>> from origamy_sim import Crease, Mesh, Settings, Sheet, Simulation
>>> side = 0.15  # a 15 cm square of kami
>>> mesh = Mesh([[0, 0], [side, 0], [side, side], [0, side]], [[0, 1, 2], [0, 2, 3]])
>>> mesh.dihedrals  # the diagonal, from vertex 0 to 2, with vertex 3 on its left and 1 on its right
array([[0, 2, 3, 1]])
>>> sheet = Sheet(mesh)
>>> sheet.set_crease((0, 2), Crease(stiffness=0.01))
>>> sheet.set_rest_angle((0, 2), 1.0)
>>> simulation = Simulation([sheet], Settings(gravity=(0, 0, 0), time_step=float("inf")))
>>> simulation.hold(0, [0, 1, 2])
>>> simulation.step().converged
True
>>> print(f"{simulation.fold_angles(0)[0]:.6f} rad")
1.000000 rad

```

## Parameters

Every parameter is a frozen pydantic model that validates its values: `Material`, `Crease` and `Settings`, with the models they hold.
Defaults are kami's, or the solver's documented values.
`Config` reads the settings and the material from a YAML file and the command line, through pydantic-settings:

```yaml
settings:
  time_step: .inf
  table:
    height: 0.0
material:
  thickness: 1.0e-4
```

```python
from pydantic_settings import CliApp, SettingsConfigDict

from origamy_sim import Config


class Scene(Config):
    model_config = SettingsConfigDict(yaml_file="scene.yaml")


config = CliApp.run(Scene)  # --material.thickness 2e-4 overrides the file
```
