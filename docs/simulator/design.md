# Simulator

The simulator origamy-sim models how sheets of paper bend, crease, stack, slide and hold, under hands, tools, gravity and the table.
The simulator knows nothing of origami steps.
The exact folding compiler, origamy, uses it to make each model physical: its 3D shape, its motion between steps, and the hands folding it.

The design below derives the simulator from the physics of paper, states what it guarantees, and plans its construction.

## 1. Goals and guarantees

The simulator aims to be the best there is: the most faithful to real paper, and fast enough to fold a crane in seconds.
No other origami simulator gives its guarantees.

Five guarantees hold at every instant of every simulation, not only at the frames shown:

1. **No crossing.**
   Paper never passes through paper, a finger, a tool or the table.
1. **Real thickness.**
   The mid-surfaces of two sheets never come closer than one paper thickness.
   A stack of n layers is therefore n thicknesses tall, and a fold wraps round the layers inside it.
1. **Bounded stretch.**
   No piece of paper stretches or shrinks by more than a set strain, half a percent by default; paper bends instead.
1. **Stated convergence.**
   Each time step reaches its stated tolerance, or reports that it did not, with the residual it reached.
   The simulator never returns a wrong state silently.
1. **Determinism.**
   The same scene and the same inputs give the same result, bit for bit, on one machine, whatever the number of threads.

Four principles shape the code:

- **One model, many configurations.**
  Folding, holding, pressing, shaping and puffing are all inputs to one physical model: forces, moving bodies, held points and crease properties.
  No action has a code path of its own.
- **Everything is an input.**
  Every physical and numerical quantity is a parameter with a documented default, from the paper's thickness to a crease's yield moment and the solver's tolerance.
  A preset, such as kami, is a named set of values, never a code path.
- **Physics in SI units.**
  Lengths in meters, forces in newtons, times in seconds.
- **Exact derivatives.**
  Every energy comes with its analytic gradient and Hessian, generated from symbolic expressions and checked against finite differences.

## 2. Paper, from first principles

### 2.1 Scales

A sheet of kami, the thin paper most origami uses, sets the scales below.

| Quantity | Symbol | Kami |
| --- | --- | --- |
| Thickness | h | 0.07 mm |
| Mass per area | ρh | 60 g/m² |
| Young's modulus in the plane | E | about 4 GPa, from 2 to 7 by fiber direction |
| Poisson's ratio | ν | about 0.3 |
| Stretching stiffness | Eh | 2.8 × 10⁵ N/m |
| Bending stiffness | D = Eh³ / 12(1 − ν²) | 1.3 × 10⁻⁴ N·m |
| Length where stretching and bending balance | √(D / Eh) | 21 µm |
| Length that gravity bends | (D / ρgh)^(1/3) | 6 cm |
| Width of a crease | S₀ | about 0.5 mm |
| Stiffness of a crease | κ ≈ D / 200h | 9 mN·m/m/rad |
| Springback of a crease pressed flat | M_y / κ | about 36° |
| Deeper set of a crease held longer | (m* / κ) ln 10 per tenfold of the hold | about 5.7° |
| Friction of paper on paper | μ | 0.17 to 0.44 sliding, 0.33 to 0.61 at rest |
| Terminal velocity of a sheet falling flat | √(2ρhg / ρₐC) | 0.91 m/s |

Seven facts follow from these numbers, and each one shapes the model.

1. **Paper bends and does not stretch.**
   Stretching a 15 cm sheet by 1% costs about 560 times the energy of rolling it round a 5 cm cylinder.
   The stiffness ratio Eh L² / D is 5 × 10⁷.
   Paper therefore moves through isometries: its surfaces stay developable, with curvature in one direction only, except at creases and cone points.
1. **The problem is numerically stiff.**
   A stiffness ratio of 10⁷ defeats local solvers, such as position-based dynamics or Gauss-Seidel sweeps, which converge slowly on stiff membranes.
   A global Newton method with a direct sparse solver handles it.
1. **Gravity matters at model scale.**
   A flap longer than about 6 cm visibly sags under its own weight.
1. **Thickness is small but decides how models look.**
   The 32 layers of a crane's neck stack 2.2 mm tall, 1.5% of the sheet's side.
   That thickness rounds every fold, separates layers, and sets how far a reverse fold can turn.
1. **Creases are material, not lines.**
   A crease is a band about half a millimeter wide where the fibers have yielded.
   Within it, the paper rests curved, so the fold rests at an angle, and it springs open a little when released.
1. **Friction holds models together.**
   Locks, pockets and tucked flaps stay in place by friction.
1. **Air slows paper.**
   A sheet falling flat nears its terminal velocity of 0.91 m/s within a quarter of a second.
   The air's density ρₐ is 1.204 kg/m³, and a square plate's drag coefficient C is 1.17.

### 2.2 What the simulator reproduces

| Phenomenon | How |
| --- | --- |
| Bending of flat paper | Elastic bending energy of the sheet |
| Near inextensibility | Stiff stretching energy, capped by a strain barrier |
| Creases with memory | Plastic crease bands whose rest curvature changes under load |
| Springback and slow relaxation | Elastic recovery, and aging of the rest curvature over time |
| Thickness of stacks and folds | Contact that keeps mid-surfaces one thickness apart |
| No crossing | Barrier contact and collision-checked Newton updates |
| Locks, pockets, tucks | Friction between sheets, and between sheets and fingers |
| Sag of large flaps | Gravity |
| Paper floating down | Air drag on each patch of paper, across its surface |
| Hands, tools, the table | Moving rigid bodies that touch the paper, and grips that hold it |
| Paper grain | Orthotropic stiffness, by fiber direction |

Tearing, wetting and heat stay out of scope for now.
Wet-folding, where moisture softens paper, is a natural later extension of the material model.

## 3. The model

### 3.1 State

The sheet is a triangle mesh of its mid-surface.
Its state is the position x of every vertex in space, their velocities v, and the plastic state of every crease.
Each vertex also keeps its place in the flat sheet, the material coordinates, which define rest lengths, rest angles and fiber directions.

### 3.2 Energy

Every behavior of the paper enters as one term of a total energy:

```text
E(x) = E_stretch + E_strain-limit + E_bend + E_crease + E_yield + E_contact + E_friction + E_drag + E_loads + E_hands + E_gravity
```

| Term | Meaning |
| --- | --- |
| E_stretch | Hyperelastic membrane energy of each triangle, orthotropic, from E, ν and the fiber direction |
| E_strain-limit | A smooth barrier on the size of each triangle's strain, growing without bound as the strain nears its cap |
| E_bend | Plate bending energy of each triangle, from a discrete curvature built from its edges' dihedral angles |
| E_crease | Bending energy of crease bands, softer than the paper around them, about a rest curvature |
| E_yield | The work creases take as they yield over a time step, dissipated or stored by hardening |
| E_contact | A barrier on the distance between mid-surface primitives, offset by the thickness |
| E_friction | Smoothed Coulomb friction between paper and paper, and between paper and the table, lagged per time step |
| E_drag | Quadratic air drag on each vertex's patch of paper, across the sheet, lagged per time step |
| E_loads | Point forces on vertices, and pressure on each sheet's front along its normals from the step's start |
| E_hands | Contact with moving bodies, and the augmented Lagrangian that carries each moving hold to its target |
| E_gravity | The weight of each vertex's share of the sheet |

The real modulus governs small strains, and the direct solver absorbs the stiffness it brings.
The barrier makes strains past the cap unreachable, whatever the forces.
The barrier acts on the norm of the Green strain, which bounds every principal strain and stays smooth where principal stretches coincide.

The bending energy is the Kirchhoff plate energy, D/2 [(tr K)² − 2(1 − ν) det K] per area, with K a curvature per triangle.
Each triangle's K comes from the normals at the midpoints of its edges, which tilt by half the dihedral angle there.
The resulting curvature is exact for a grid rolled round a cylinder through its vertices.
A boundary edge has no neighbor, so the plate reads the paper as flat across it.
A triangle on a free edge therefore bends through its interior edges alone.
The older energy on each interior edge alone depends on the mesh's directions, by up to a factor of 3 on a regular grid.
Every dihedral has a rest angle, zero by default, and the plate's rest curvature comes from those angles alike.
Paper can therefore rest curved, as a sheet taken off a roll does, and a crease rests at its dihedral's angle.

### 3.3 Motion: forces through an incremental potential

Forces act on the paper at every instant of a fold, as in real hands.
The simulator integrates Newton's laws implicitly through the incremental potential of a time step:

```text
x(n+1) = argmin over x of  ½ (x − x̂)ᵀ M (x − x̂) / Δt²  +  E(x),   with  x̂ = x(n) + Δt v(n)
```

Minimizing this potential solves one time step of implicit Euler.
Inertia, elastic forces, contact, friction, drag, gravity and the hands' forces all balance at its minimum.
Large time steps stay stable, and every force term keeps its physical meaning.
Dropping the inertia term gives quasi-static motion, for folds slow enough that inertia does not matter.

Energy minimization is therefore how forces are applied, not a replacement for them.
A fold is a sequence of forces and moving contacts over time, and the paper responds at every time step.

### 3.4 Creases

Measurements describe a crease as a band of width S₀, about half a millimeter, whose rest curvature integrates to the crease's rest angle.
The simulator models it the same way:

- **Geometry.**
  A crease band is a few parallel lines of mesh edges along a crease line, a tenth of a millimeter apart.
- **Elasticity.**
  Bending within the band costs a reduced stiffness, about its rest curvature.
- **Plasticity.**
  When the bending moment in the band passes a yield moment, the rest curvature moves toward the current curvature.
  Hardening raises the yield moment with the rotation the crease has yielded and crept through.
  Pressing a fold flat therefore makes it rest more closed; releasing it lets it spring back to its rest angle.
  The springback is the yield moment over the stiffness, 36° for paper.
- **Aging.**
  Below the yield moment, the rest curvature creeps toward the current one at the rate ω sinh(m / m*), as thermally activated flow does.
  A held fold therefore loses moment logarithmically in time, and sets deeper the longer it is held.
  Quasi-static steps span no time, so creases yield in them but do not age.
- **A precrease.**
  A crease folded flat, then pressed flat again, yields both ways.
  Released, it rises by its springback toward the side it was folded to.
  The crease stays softer than the paper around it, so the paper refolds there first.

At coarse resolution, a crease band narrows to a sharp crease on mesh edges, with the same rest angle and an equivalent stiffness.
Both levels share one definition, so a model can run coarse for speed and fine for close views.
The lines of a band of n turn in series, each by an n-th of the band's turn, under one moment.
Each line therefore takes n times the crease's stiffness and hardening, and an n-th of its aging rate.
Each line rests at an n-th of the crease's rest angle.
The band then turns, yields and ages exactly as its sharp crease does, and wraps round the layers inside it as a sharp crease cannot.

A time step yields each crease by the variational update of plasticity.
Within the step, a crease's energy is the least, over how far it yields, of its elastic energy and the work that yielding takes.
That energy is convex and continuously differentiable in the fold angle, with the moment as its slope.
Newton's method therefore yields creases inside each time step, with exact derivatives, and the step takes the new rest angles.

### 3.5 Contact with thickness

Contact is the hardest part of simulating origami, and the part where the simulator innovates most.

**The barrier.**
Each pair of nearby mid-surface primitives, a point and a triangle or two edges, adds a barrier b(d − h) on their distance d.
The barrier is zero beyond an activation distance, smooth, and grows without bound as d nears the thickness h.
Mid-surfaces therefore never come closer than h, as in codimensional incremental potential contact.

**Finding pairs.**
A bounding volume hierarchy over the triangles, and another over the edges, find the pairs whose boxes overlap.
Each primitive's box grows by its half thickness and half the gap, so boxes overlap wherever the barrier may act.
Each hierarchy is built at the start of a time step and refit to every later arrangement, since paper keeps its neighbors as it moves.
A query costs about the logarithm of the count plus what it finds, however far the primitives sweep.

**Parallel edges.**
Two parallel edges have no single closest pair of points, so their distance is not smooth there.
The barrier between two edges fades smoothly to zero as they turn parallel, below a set angle, as in incremental potential contact.
The points and triangles around them hold them apart there instead.

**Collision-checked updates.**
Each Newton update is cut short before any primitive would cross the offset surface of another, by continuous collision detection over the whole update.
The detection is conservative: it may stop an update early, but it never misses a crossing.
Every state the solver visits, not just every state it returns, is free of crossings.

**Pairs that are not contact.**
The two sides of a fold meet within the fold's own band.
Pairs closer along the sheet than the crease band's width do not take the barrier, since their distance is the bend itself.

**Folded states come from folding.**
Paper folded flat has coincident layers, at distance zero, which every barrier forbids.
The simulator never starts from such a state.
Every model starts as the flat sheet, and each step of its fold sequence moves the physical state the step before left.
Each fold wraps the layers inside it as real paper does, so the stack's thickness comes from the folding itself.
At every step, the layer order wherever paper overlaps must match the order origamy's kernel proves.

**Faster contact.**
Offset geometric contact bounds how far each vertex may move without any crossing, with no collision detection over the update.
The simulator evaluates it as a faster filter in phase 7, and keeps the guarantee whichever filter it uses.

### 3.6 Friction and air

**Friction.**
Friction follows incremental potential contact, lagged per time step.
At the start of a time step, each contact fixes its normal force λ, its tangent plane and its closest points.
Over the step, friction resists the slip u of those points in that plane with the potential μλ f₀(|u|).
Past a stick distance ε, f₀ is the slip itself, so a sliding contact feels the force μλ against its slip.
Below ε, f₀ turns smoothly, so a contact that friction holds creeps by less than ε a step.
The table holds each vertex the same way, with a friction coefficient of its own.
The friction's Hessian is exact and positive semi-definite.

**Lags.**
A normal force fixed at the start of a step lags its load by a step.
A sheet tilted on the table thus keeps its old weight for that step.
Each time step therefore lags friction again from the solution it reached, and solves once more.
The lags end once a solve converges where it starts, or after a set count, three by default.

**Air.**
Air drags each vertex's patch of paper against its velocity v across the sheet, with the force ½ ρₐ C A v².
The potential k |w·n|³ / 3 of the vertex's move w over the step gives that force, with k = ρₐ C A / 2Δt².
The normal n is the patch's normal at the start of the step, so the potential is convex and its Hessian exact.
Drag along the sheet, from skin friction, is a few percent as strong at the same speed, and left out.
Quasi-static steps have no velocity, and so no drag.

### 3.7 Hands, tools and forces

Folding by hand is a choreography of a few physical actions.
The simulator provides each as an input to the same model:

| Input | Physics | Uses |
| --- | --- | --- |
| Rigid bodies on paths | Moving shapes, such as a fingertip capsule, a bone folder or the table, that touch paper through the contact barrier and friction | Lifting a flap, pushing a reverse fold in, pressing a crease flat |
| Grips | Holds that tie a patch of paper to a moving body, made and released over time | Pinching a corner, carrying an edge over |
| Holds | Held vertices fixed where they lie, or carried to a target over each time step | Holding a corner, carrying a flap over |
| Forces | Point forces and pressure, constant or varying over time | Pulling a point toward a target, flattening a stack |
| Crease actuation | A crease's rest curvature and stiffness, set over time | Shaping a model, or self-folding sheets |
| Gravity | A uniform field | Sag, flaps falling open |

A finger pressing along a fold line yields the crease band there, so creasing comes from pressure, as it does by hand.
Hands appear in the animations, so a person sees exactly how to fold.

**Moving holds.**
A hold moves its vertex within a time step, so collision detection covers the vertex's path, and the paper around it follows.
The vertex joins the solve, pulled to its target by the augmented Lagrangian of incremental potential contact.
A stiff spring pulls the vertex, and each round of minimization raises a force on every hold short of its target.
The step converges only once every hold lies within the tolerance of its target.
A hold that contact blocks, such as a hand pushing paper into the table, never converges, so the step halves and then reports the failure.
Each hold reports the force it exerts: the force that balances every other force on its vertex.

**Rigid bodies.**
A body is a sphere round one point, or a capsule round the segment between two, of a given radius.
Its points join the solve as moving holds, so a hand carries a body exactly as it carries paper.
Paper keeps its half thickness plus the body's radius from the body, through the same barrier, friction and collision detection as paper on paper.
The pairs are the body's points against the paper's triangles, and a capsule's axis against the paper's edges and points.
Each body has its own coefficient of friction, and reports the force it exerts on the paper.
A hand may hold a body by a spring, as a finger pad yields when it presses.
The body then presses with the spring's stiffness times how far its path runs past where the paper stops it.
The spring presses on while the hand stays, until the hand moves again.
A finger pad of 14 mm radius and modulus 36 kPa yields at about 700 N/m under a few newtons, from Dzidek et al., 2017.
A rigid path instead presses stacked paper with whatever force its height demands, which a micrometer can make a thousand newtons.

**Grips.**
A grip ties paper vertices to a body, as a pinch does, and the body carries them as it moves.
The gripped vertices are moving holds, so the augmented Lagrangian carries them within each time step.
A sphere carries its grip as its center moves.
A capsule keeps each gripped vertex's share of the way along its axis.
The vertex's offset from the axis turns by the least rotation from the old axis to the new.
A released vertex falls free with the velocity the body gave it.
A body's force sums its contact with the paper and its grip on it.
A grip follows the hand, so a pad's spring lets the body yield without dragging the grip along.

### 3.8 Solver

Each time step minimizes the incremental potential with a projected Newton method:

- **Hessian.**
  Each energy term contributes its exact Hessian.
  Where their sum is not positive definite, each element's Hessian takes the absolute value of its eigenvalues, so every Newton direction descends.
  Flipping negative curvature, rather than clearing it, keeps its size in the step and converges faster.
- **Trust radius.**
  No Newton update moves a vertex farther than a trust radius in meters.
  The radius doubles each time an update it shortened is taken whole.
  Where some motion costs nearly no energy, as for a sheet held along one line, the update would otherwise run off by millions of meters.
  Where some motion costs no energy at all, the Hessian is singular, and a Levenberg–Marquardt shift of |g|∞ / r takes its place.
  The shift sends that motion down the gradient g by at most the trust radius r.
  A gradient within rounding of zero leaves the step no unique result, and the step reports it.
- **Linear solve.**
  A sparse Cholesky factorization solves each Newton system, with its symbolic analysis reused while the contact set keeps its pattern.
- **Line search.**
  Backtracking from the longest update that collision detection allows, until the energy decreases enough.
  Each fraction of the update is tried as it is, then retracted: refitted, as rigid as possible, to its triangles' rotations.
  The retraction turns paper without stretching it, about the held vertices where the update puts them.
- **Convergence.**
  The largest vertex move of the Newton update falls below a tolerance in meters.
- **Recovery.**
  A time step whose solve does not converge retries as two time steps of half its length.
  A time step that would halve below the shortest time step reports that it did not converge.
  The report counts the time steps taken.

The solver is the reference: accurate, robust and the one every guarantee is proved for.
A second solver, vertex block descent with offset contact, joins in phase 7 for interactive speed on GPUs, minimizing the same energy.

### 3.9 Discretization

- **Meshes that follow creases.**
  A constrained Delaunay triangulation of the crease pattern makes every crease line a chain of mesh edges.
  Ruppert's refinement then leaves no angle under 20° and no triangle over an area bound, splitting creases where it must.
  The spade crate builds both, and each triangle carries the face of the crease pattern it lies in.
- **Grading.**
  Elements shrink toward creases, where curvature concentrates, and grow across flat faces.
- **Two levels of detail.**
  Sharp creases on a coarse mesh run fastest; crease bands on a graded mesh look and behave most like paper.
- **Precision.**
  Every quantity is a 64-bit float, and collision detection bounds its own rounding errors.

### 3.10 Parameters

Every quantity below is an input, in SI units, with a documented default.
A preset fills them all for one paper: the kami preset takes the values of section 2.1.
Phase 4 calibrates the crease parameters on published measurements.

| Group | Parameters |
| --- | --- |
| Sheet | Thickness, mass per area, Young's modulus along and across the grain, shear modulus, Poisson's ratio, grain direction |
| Strain limit | The strain cap, the fraction of it where the barrier starts, the barrier's energy per volume |
| Dihedral | Rest angle, for every interior edge, crease or not |
| Crease | Stiffness, yield moment, hardening, aging moment and aging rate; a band's lines take their share of each |
| Contact | The barrier's activation gap and energy scale, the fade of parallel edges, friction of paper on paper and on each body, the stick distance, the exclusion radius along the sheet |
| Bodies and grips | Shapes, radii, friction, the stiffness of the hand, paths, and the times grips close and open |
| World | Gravity, the table and its height and friction, the air's density and drag coefficient |
| Solver | Time step, the shortest time step, the lags of friction, the stiffness and rounds of moving holds, tolerance in meters, iteration limit, trust radius, sufficient decrease, backtracking factor, shortest update |

Each crease takes its own values, so one model can hold sharp and soft creases, or two papers in one sheet.

In Python, each group is a frozen pydantic model.
Field constraints check each value, and model validators check the conditions that tie values together, such as a stiffness that stays positive definite.
The core checks every value again, so Rust callers and unvalidated models meet the same rules.
A property test holds the two checks to one verdict.
A pydantic-settings configuration reads the parameters from a YAML file and the command line, which overrides the file.

### 3.11 Outputs

| Output | Content |
| --- | --- |
| States | Vertex positions and velocities, crease rest curvatures, and the force each hold exerts, as NumPy arrays |
| Time step reports | Newton iterations, time steps taken, residual, every energy term, greatest strain, least distance between sheets, contacts, and whether the time step converged |
| Surfaces with thickness | Both faces of the sheet, offset by half the thickness, and its rims, for rendering |
| glTF 2.0 | Meshes and animations, the standard format for 3D |
| FOLD 1.2 | Frames with 3D vertex coordinates and fold angles, for origami tools |

## 4. What is new

Existing tools each solve part of the problem.

| Tool | Approach | Missing |
| --- | --- | --- |
| Origami Simulator | Bar-and-hinge springs on the GPU, all creases at once | Contact, thickness, plasticity; it lets paper cross paper |
| MERLIN | Bar-and-hinge analysis with nonlinear solvers | Contact and folding sequences |
| SWOMPS | Bar-and-hinge with barrier contact and sequential loading | Thin-sheet bending, crease plasticity, hands, speed |
| IPC, C-IPC, OGC | Guaranteed crossing-free contact for shells and cloth | Paper's creases, plasticity, and fold sequences |

The simulator combines and extends them:

1. Guaranteed crossing-free contact at paper's real thickness, for stacks of dozens of layers.
1. Every model folded from the flat sheet, step by step, each step's layer order checked against origamy's exact one.
1. Creases as plastic bands, with yield, springback and aging, reducing exactly to sharp creases at coarse resolution.
1. Hands, tools and grips as ordinary physical inputs, so folds are performed rather than prescribed.
1. A certificate of physical feasibility: a fold sequence the simulator performs is one real paper allows, without crossing or tearing.
1. Sensitivities of the final shape to forces and crease angles, by the adjoint method, so agents can optimize a model's look.

## 5. Architecture

### 5.1 Packages

The simulator is a package of the origamy repository, as origamy-mcp is.

| Package | Holds |
| --- | --- |
| origamy-sim | The simulator: a Rust core and its Python package |
| origamy | The exact folding kernel, fold scripts, the command line and the viewer, depending on origamy-sim |
| origamy-mcp | The MCP server |

One commit can change the simulator and origamy together, under one CI and one lock file.
The simulator stays general all the same: its interface speaks of meshes, materials, bodies and forces, never of folds or steps.

### 5.2 Language and libraries

The core is Rust, with Python bindings, as in ruff, uv, polars and pydantic-core.

| Choice | Reason |
| --- | --- |
| Rust | Compiled speed, memory safety, fearless parallelism and reproducible builds |
| PyO3 and maturin | The standard bindings and build for Rust in Python, with wheels for every platform |
| faer | State-of-the-art dense and sparse linear algebra in Rust, including sparse Cholesky |
| nalgebra | Small fixed-size vectors and matrices for per-element derivatives |
| rayon | Data parallelism with deterministic reductions |
| SymPy | Symbolic energies, from which the derivative code is generated |

NumPy alone is fast only for large, regular array operations.
Contact search, collision detection, sparse assembly and line searches are irregular and branching, so in NumPy they run as Python loops, a hundred times slower.
JAX compiles and differentiates array programs, but contact sets change size every time step, collision detection branches, and JAX has no fast sparse direct solver.
JAX suits learned components and experiments later, not the core.

### 5.3 Layout

```text
Cargo.toml                              the Rust workspace, beside pyproject.toml's uv workspace
packages/origamy-sim/
  crates/origamy-sim/                   the core: geometry, energies, contact, solvers; no Python
  crates/origamy-sim-py/                PyO3 bindings, built into the Python package
  crates/origamy-sim/tests/             physics benchmarks, with references and acceptance thresholds
  crates/origamy-sim/benches/           Criterion benchmarks of time steps
  src/origamy_sim/                      the Python package: the core's classes, the parameter models, the configuration
  codegen/                              SymPy sources of every energy and the generator of their derivatives
  tests/                                the Python tests
docs/simulator/                         this design, the recorded runs and the Python API
docs/simulator.py                       the recorder of the runs the docs show
```

### 5.4 Core modules

| Module | Holds |
| --- | --- |
| `mesh` | Rest geometry: triangles, edges, dihedrals and the boundary |
| `parameter` | The checks of parameter values, each naming the parameter it refuses |
| `material` | Thickness, weight, orthotropic stiffness, the strain limit, and presets |
| `settings` | Gravity, the time step, contact and friction, the table, the air and the solver |
| `sheet` | A mesh with its material, creases and rest angles, and the rest data its energies read |
| `energy` | One module per energy term, each with value, gradient and exact Hessian |
| `generated` | Derivative code generated from symbolic expressions |
| `system` | The sparse Newton system in 3 by 3 blocks, and its Cholesky factorization |
| `simulation` | Sheets, settings and held vertices; time steps by projected Newton, and their reports |
| `contact` | Bounding volume hierarchies, distances, the barrier and its fade for parallel edges, friction, exclusions, continuous collision detection |
| `body` | Spheres and capsules on paths, and their parameters |
| `faces` | Meshes of a crease pattern's faces, refined, with the face of each triangle |

### 5.5 Engineering standards

- **Lints.**
  The Rust code passes rustfmt and clippy, with clippy's pedantic group as errors, and the core holds no unsafe code.
  Python follows origamy's toolchain: ruff with every rule, ty with every rule as an error, rumdl.
- **Tests.**
  Unit tests for every function, property tests with proptest and Hypothesis, and finite-difference checks of every gradient and Hessian.
- **Invariants.**
  Debug builds assert every guarantee after every time step; release builds report them.
- **Benchmarks.**
  Criterion benchmarks cover hot paths, and CI tracks end-to-end timings.
- **Validation.**
  Physics benchmarks with references, run in CI, each with an acceptance threshold.
- **Documentation.**
  Every public item documented; a guide and an API reference, as for origamy.

## 6. Validation: what excellent means

The simulator is excellent when it passes all of these, in CI:

| Benchmark | Reference | Threshold |
| --- | --- | --- |
| Gradients and Hessians of every energy | Finite differences | Relative error under 10⁻⁶ |
| Continuous collision detection | Exhaustive adversarial cases, and a published benchmark | No missed crossing |
| Paper rolled round a cylinder | The cylinder | Exact curvature on a grid through its vertices |
| A cantilever under gravity | The elastica | Tip deflection error under 0.1% |
| Miura-ori folding | Rigid origami | Greatest strain under 10⁻⁸, fold angles within 10⁻⁶ rad |
| Paper curved at rest | Its rest shape | Every vertex within 1 nm |
| A square hung on a hinge | Hanging straight | Every vertex within 1 µm |
| The assembled system | Finite differences of the whole potential | Gradient within 10⁻⁶ and Hessian within 10⁻⁵, with and without contact |
| Sheets dropped on the table | k thicknesses for layer k | Each layer within the barrier's gap of its place; distances never under h |
| A strip folded over itself onto the table | No crossing | Distances never under h at any step |
| A square sliding down an incline, on the table and on paper | Coulomb's law: acceleration g (sin θ − μ cos θ) | Velocity within 1% |
| A square on an incline gentler than its friction angle | Coulomb's law: no slip | Slip under the stick distance a step |
| A sheet falling flat through air | Implicit Euler of v′ = −g + kv², and the terminal velocity | Velocity within 10 nm/s of implicit Euler, and within 0.1% of the terminal velocity |
| A fold pressed flat, then released | Measured springback | Opening between 30° and 40°, the measured scatter |
| A fold turned short of or past its yield angle, then released | Perfect plasticity | Springs back whole, or by its yield angle, within 10⁻⁶ rad |
| A precrease pressed flat again, then released | Perfect plasticity | Rises by its yield angle, within 10⁻⁶ rad |
| A crease band pressed fully flat, then released | Its sharp crease, and paper's 36° | Opening within 10⁻⁶ rad of the sharp crease's springback |
| A fold held for 1, 10 and 100 s, then released | Logarithmic aging, and paper's 5.7° a decade | Springback within 0.002 rad of the law; a decade within 5% of 5.7° |
| A sheet hanging from a hand, then lifted | Statics: the hand carries the weight | The hand reaches its target within 10⁻¹² m, and carries the weight within 10⁻⁶ |
| A sheet pulled across the table by a hand | Coulomb's law: the friction coefficient times the weight the table bears | Pull within 1% |
| A hand pushing paper into the table | No crossing | The step reports that it did not converge, with paper above the table |
| A fingertip pressing paper on the table, then sliding | Coulomb's law: the paper follows a fingertip whose friction beats the table's | The paper moves within 1% of the fingertip, or stays within 1% |
| A finger pad's hand running 1 mm past paper on the table, then staying | Hooke's law: the pad's stiffness times its overrun | The push within 10⁻⁶ of the pad's spring, and within the barrier's gap of 0.7 N |
| A fingertip folding kami in half, then creasing it with 1 N and with 4 N | Measured springback, and a crease that does not harden | Each fold opens between 30° and 40°, the two within 1°, every step converged |
| Pressure pulling a sheet with its weight per area, and twice that | Newton's laws | The sheet stays still, or rises at g, within 10⁻⁷ m/s |
| A sheet folded over n layers | n thicknesses | Stack height within 1%, distances never under h |
| Every time step of every benchmark | The five guarantees | Always |
| The crane, folded by hand from the square | origamy's exact layer orders | Same layer order at every flat state, no crossing |

Performance targets, on one laptop core:

| Task | Target |
| --- | --- |
| A quasi-static time step of a crane-sized mesh, about 5,000 vertices | Under 100 ms |
| Animating one fold, 60 frames | Under 5 s |

## 7. Plan

Each phase ends when its exit tests pass in CI.

| Phase | Builds | Exits when |
| --- | --- | --- |
| 0. Foundations | Package, Rust workspace, Python package, bindings, CI, the rest mesh and its topology | A mesh built from Python reports its edges, dihedrals and boundary, and every check passes in CI |
| 1. The elastic sheet | Membrane, strain barrier, bending, sharp creases, gravity, held vertices, projected Newton, sparse Cholesky, derivative generation, the validation harness | Derivative, bending, cantilever and Miura benchmarks pass |
| 2. Contact | Bounding volumes, distances, thickness barrier, exclusions, continuous collision detection, the table | A sheet folds over itself under gravity onto the table with no crossing, and the stack benchmark passes |
| 3. Friction and dynamics | Lagged friction, air drag, adaptive time steps | Sliding, sticking and falling tests match theory |
| 4. Paper | Crease bands, plasticity, aging, orthotropy, presets calibrated on published measurements | The springback benchmark passes |
| 5. Hands | Rigid bodies on paths, grips, forces, pressure | A fingertip folds a sheet in half and creases it, repeatably |
| 6. origamy on the simulator | Meshes from crease patterns, a choreography of hands for each action, the viewer on simulated frames | The gallery folds physically end to end, each step's layer order matching origamy's, and the old rigid 3D code is deleted from origamy |
| 7. Speed and reach | Profiling, SIMD, offset contact filter, a GPU solver, adjoint sensitivities | Performance targets met; sensitivities pass finite-difference checks |

Phases 0 to 5 are done.
Phase 6 has begun: `origamy.physics` folds the preliminary and waterbomb bases from the flat sheet, each layer in origamy's order.
Two findings from phase 1 shape the next phases:

- **Clamps.**
  Holding two rows of vertices clamps a sheet half a cell inside the second row.
  The curvature along an edge spreads over the triangles on both its sides.
  The cantilever benchmark measures from there, and converges at second order in the cell size.
- **Large turns in one step.**
  A Newton update moves vertices along straight lines, so turning a flap stretches it.
  The line search alone then shortens the update to keep the paper's length, and a turn of a radian took 136 Newton iterations.
  The line search now also tries the update's retraction: its positions refitted, as rigid as possible, to its triangles' rotations.
  The same turn takes 6 iterations, and a turn of 172° takes 9.
- **Exact Hessians.**
  Projecting every element's Hessian made Newton converge only linearly, as the projections drop curvature the sheet has.
  The solver now projects only when the exact Hessian is not positive definite.
  A time step of a falling strip takes 4 iterations instead of 22, and a quasi-static fold increment 7 instead of 128.

Phase 2 adds these findings:

- **Every term in the total.**
  The line search once summed every energy term but contact, so a sheet landing on the table stalled a micrometer above it.
  The total now names every term, so the compiler refuses a total that leaves one out.
  Two tests check the assembled gradient and Hessian against finite differences of the whole potential, with and without contact.
- **Updates out of reach.**
  A flat square held along one line of vertices swings at no cost, so its first quasi-static update ran 5 × 10⁸ m.
  The uniform grid that found contact pairs then ran out of memory.
  The trust radius now bounds every update, and bounding volume hierarchies replace the grid.
- **Absolute eigenvalues.**
  Flipping each element's negative eigenvalues instead of clearing them cut quasi-static iterations by 13% to 24% across the benchmark scenes.
- **Far from rest.**
  Quasi-static steps far from rest still converge linearly while the Hessian stays indefinite.
  A square of 24 by 24 cells, clamped along one side, sags from flat to rest in about 130 iterations.

Phase 3 adds these findings:

- **Lagged normal forces.**
  A square tilted on the table first slid 1.2% to 2.5% slower than Coulomb's law says.
  Its friction kept the normal force of each step's start, from before the load changed.
  Lagging friction again from each step's solution brings the square within 1%.
- **Drag on stacks.**
  Air drags every layer of a stack as if it were alone, so a stack falls as fast as one sheet.
  Real air reaches only the outer layers, so a stack of n layers falls √n times faster.
  Shielding the layers that contact covers is left for later.

Phase 4 adds these findings:

- **How far a sharp crease closes.**
  Pressed flat, the cells beside a sharp crease meet a thickness apart where their diagonals cross, 2.5 mm from the crease in 5 mm cells.
  The crease therefore stops about 0.03 rad short of flat.
  Pressed to 0.04 rad short, kami's crease springs back to an opening of 38°.
  A crease band of five lines 0.1 mm apart wraps round instead: pressed fully flat, it springs back to 36°.
- **Aging's one measurement.**
  Farain's constant for paper, 5.7, comes without a unit, and the simulator reads it as degrees per tenfold of the hold.
  Read per factor of e instead, folds of kami would set 13° deeper a decade.
  A measurement of paper's moment relaxation would settle the aging moment.

Phase 5 adds these findings:

- **Singular quasi-static steps.**
  Paper hanging flat from a straight line is free to turn about it, so the Hessian is singular there.
  Newton's update then depended on rounding, and failed on one mesh of four.
  A Levenberg–Marquardt shift now sends the free motion down the gradient by at most the trust radius.
- **Retracting about holds.**
  The retraction once let moving held vertices drift, so a grip's first quasi-static step took 674 Newton iterations.
  Pinning every held vertex where the update puts it brings that step to 19.
- **No thickened starts.**
  Lifting a flat fold's layers apart by convex optimization stacks the preliminary base, but not the petal fold.
  There a layer ends exactly at the crease of a fold that wraps it, so the fold's two halves would part at one line.
  Real paper bends round the layer's edge within a fraction of a millimeter, which 5 mm cells cannot follow.
  Every model is therefore folded from the flat sheet, step by step.

Phase 6 adds these findings so far:

- **Still faces stay still.**
  The rigid path of the preliminary base's collapse once turned the flat crease between its two fixed faces.
  Holding both faces then fought the path, and folds lagged by 6°.
  The path now keeps every crease between faces that keep their places.
  Without gravity or a table, the collapse then follows the path with 10⁻¹⁴ J of elastic energy.
- **Kami sags.**
  A 7.5 cm flap of kami droops about 2 cm under its own weight.
  On a table, the sag sent collapses down other branches, such as a valley pressed flat into the table.
  Hands now carry the weight of the paper a fold moves, then let go, and the paper settles under gravity.
- **Flat paper branches.**
  Every fold from flat paper starts where several ways to fold meet.
  With even rates, a first turn of 6° pressed a valley of the waterbomb base flat into the table.
  Turns now ease in and out as a cosine, so the first of 30 increments turns half a degree.
- **Drives that press too hard.**
  Drives a thousand times as stiff as kami's crease pressed closing layers so hard that Newton stalled.
  A hundred times keeps creases within 0.2° of their path, and every increment converges.

Phase 6 deletes most of origamy's rigid 3D layer.
The layer comprises per-face rigid forms, the shape, flex and tilt mechanisms, layer lifts, drawn bends, rigid collapse paths and the viewer's animation modes.
Shaping becomes crease actuation and hands; animation becomes simulated motion.
The rigid collapse paths stay, because the simulated folds follow them.

A research track runs alongside, with ambitious targets:

- **Creases that form where paper is pressed,** by plastic localization and adaptive remeshing, so a model need not name its crease lines in advance.
- **Choreographies found by optimization,** so hands for any move come from the move itself.
- **Curved folds and wet-folding,** for the models that need them.

## 8. Decisions

| Decision | Chosen | Alternatives |
| --- | --- | --- |
| Time stepping | Implicit Euler as an incremental potential | Explicit integration, which needs tiny time steps for stiff paper |
| Reference solver | Global projected Newton with sparse Cholesky | Local block descent, fast on GPUs but slow to converge on stiff paper |
| Contact | Thickness barrier with conservative collision-checked updates | Penalty springs, which let paper cross under load |
| Inextensibility | Real modulus plus a strain barrier | Very stiff springs, ill conditioned; hard constraints, brittle |
| Creases | Plastic bands, reducing to sharp creases | Sharp creases only, with no width at the fold and no springback |
| Folded states | Folding from the flat sheet, step by step, as paper is folded | Thickening a flat fold into a stack, which cannot wrap a layer that ends at a fold's crease on a coarse mesh |
| Fold paths | Creases driven along origamy's rigid folding path, where one exists | Creases turning at even rates, which bend faces off any rigid motion |
| Weight while folding | Hands carry the weight of the paper a fold moves | Paper lifted only by its creases, which sags kami into other folds |
| Mountain folds | Folding up from the turned-over sheet, then turning it back | Folding down into the table, which the table forbids |
| Meshes | A constrained Delaunay triangulation of the crease pattern, refined by Ruppert's algorithm | Regular grids, whose edges cross creases at arbitrary angles |
| Core language | Rust | C++, NumPy, JAX |
| Bending | Plate energy of a curvature per triangle, from mid-edge normals | Energy per interior edge, which depends on the mesh's directions |
| Derivatives | Generated from symbolic energies | Hand derivation, error prone; runtime autodiff, slow in the hot loop |
| Friction | Lagged, smoothed Coulomb friction in the incremental potential | Friction forces applied after each step, which stick and slip at large time steps; exact Coulomb constraints, which are not smooth |
| Air | Quadratic drag across the sheet, as a potential | Damping proportional to mass, which slows paper alike in every direction, even in a vacuum |
| Crease plasticity | The variational update, inside each time step | A return mapping after each step, which lags yielding by a step |
| Crease aging | Thermally activated creep, Eyring's law, in the same update | A spectrum of relaxation times, which ages a briefly pressed fold only over the press's own time |
| Moving holds | The augmented Lagrangian inside each time step, as in incremental potential contact | Moving held vertices between steps, which collision detection cannot follow |
| Rigid bodies | Spheres and capsules whose points are moving holds | Signed distance fields, general but without exact derivatives at sharp features |
| Pressing | A spring of the finger pad's stiffness between a body and its path | A rigid path, whose force on stacked paper jumps with its height |
| Grips | Moving holds that follow the body's motion | Springs between paper and body, which let a pinch slip under load |

## 9. References

- [Fast, Interactive Origami Simulation using GPU Computation](https://erikdemaine.org/papers/OrigamiSimulator_Origami7/paper.pdf), by Amanda Ghassaei, Erik Demaine and Neil Gershenfeld, Origami⁷, 2018.
- [Bar and hinge models for scalable analysis of origami](https://collaborate.princeton.edu/en/publications/bar-and-hinge-models-for-scalable-analysis-of-origami/), by Evgueni Filipov, Ke Liu, Tomohiro Tachi, Mark Schenk and Glaucio Paulino, International Journal of Solids and Structures, 2017.
- [SWOMPS, the Sequentially Working Origami Multi-Physics Simulator](https://drsl.engin.umich.edu/software/swomps-package), by Yi Zhu and Evgueni Filipov.
- [Discrete Shells](https://diglib.eg.org/handle/10.2312/SCA.SCA03.062-067), by Eitan Grinspun, Anil Hirani, Mathieu Desbrun and Peter Schröder, Symposium on Computer Animation, 2003.
- [Computing discrete shape operators on general meshes](https://doi.org/10.1111/j.1467-8659.2006.00973.x), by Eitan Grinspun, Yotam Gingold, Jason Reisman and Denis Zorin, Computer Graphics Forum, 2006.
- [Incremental Potential Contact](https://research.adobe.com/publication/incremental-potential-contact-intersection-and-inversion-free-large-deformation-dynamics), by Minchen Li, Zachary Ferguson, Teseo Schneider, Timothy Langlois, Denis Zorin, Daniele Panozzo, Chenfanfu Jiang and Danny Kaufman, ACM Transactions on Graphics, 2020.
- [Codimensional Incremental Potential Contact](https://research.adobe.com/publication/codimensional-incremental-potential-contact), by Minchen Li, Danny Kaufman and Chenfanfu Jiang, ACM Transactions on Graphics, 2021.
- [Offset Geometric Contact](https://graphics.cs.utah.edu/research/projects/ogc/), by Anka He Chen, Jerry Hsu, Ziheng Liu, Miles Macklin, Yin Yang and Cem Yuksel, ACM Transactions on Graphics, 2025.
- [Coefficient of friction of paper used in a copy machine](https://instron.jp/ja-jp/-/media/literature-library/applications/2006/03/coefficient-of-friction-of-paper-used-in-a-copy-machine.pdf), Instron application note, 2006.
- [Stick-slip dynamics in an interleaved system with self-amplified friction](https://arxiv.org/abs/2602.14935), by Plati, Restagno and Poulard, arXiv, 2026.
- Fluid-Dynamic Drag, by Sighard Hoerner, 1965.
- [The mechanical response of a creased sheet](https://arxiv.org/abs/1404.1243), by Frédéric Lechenault, Benjamin Thiria and Mokhtar Adda-Bedia, Physical Review Letters, 2014.
- [Relaxation constant in the folding of thin viscoelastic sheets](https://arxiv.org/abs/1905.09220), by Kasra Farain, Physical Review Applied, 2020.
- [The variational formulation of viscoplastic constitutive updates](https://resolver.caltech.edu/CaltechAUTHORS:20171213-091546263), by Michael Ortiz and Laurent Stainier, Computer Methods in Applied Mechanics and Engineering, 1999.
- [Relaxation Mechanisms in the Unfolding of Thin Sheets](https://journals.aps.org/prl/abstract/10.1103/PhysRevLett.107.025506), by Benjamin Thiria and Mokhtar Adda-Bedia, Physical Review Letters, 2011.
- [Plasticity and Aging of Folded Elastic Sheets](https://arxiv.org/abs/2004.11825), by Théo Jules, Frédéric Lechenault and Mokhtar Adda-Bedia, arXiv, 2020.
- [Local Mechanical Description of an Elastic Fold](https://arxiv.org/pdf/1808.04892), by Théo Jules, Frédéric Lechenault and Mokhtar Adda-Bedia, Soft Matter, 2019.
- [Relaxation Mechanisms in the Unfolding of Thin Sheets](https://journals.aps.org/prl/abstract/10.1103/PhysRevLett.107.025506), by Benjamin Thiria and Mokhtar Adda-Bedia, Physical Review Letters, 2011.
