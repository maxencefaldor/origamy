# Python API

The package `origamy_sim` holds the simulator's classes, which its compiled core defines, and the models of its parameters.
Every length is in meters, every angle in radians, and every array a NumPy array.

## Meshes, sheets and simulations

::: origamy_sim.Mesh

::: origamy_sim.Sheet

::: origamy_sim.Simulation

::: origamy_sim.Report

::: origamy_sim.Energy

::: origamy_sim.SingularError

## Parameters

Each parameter model is a frozen pydantic model that validates its values.
The function `copy.replace` makes a validated copy with some fields changed.

::: origamy_sim.Material

::: origamy_sim.Stiffness

::: origamy_sim.StrainLimit

::: origamy_sim.Crease

::: origamy_sim.Settings

::: origamy_sim.Solver

::: origamy_sim.Contact

::: origamy_sim.Table

::: origamy_sim.Air

::: origamy_sim.Config
