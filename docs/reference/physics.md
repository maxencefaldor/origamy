# Physics

`origamy.physics` folds a build on the simulator, origamy-sim, one step at a time from the flat sheet.
Each step turns its creases toward the angles its state gives them, and each fold ends measured against that state.
The [simulator's design](../simulator/design.md) explains the physics behind it.

```python
from origamy.physics import simulate

folding = simulate(paper.steps)
stacked = all(not stage.misordered for stage in folding.stages)
```

::: origamy.physics
    options:
      members: [simulate, Physics, Bands, Folding, Stage, PhysicsError]
