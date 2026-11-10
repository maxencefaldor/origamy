"""Print bit-exact fingerprints of simple simulations, to compare machines."""
import hashlib, math
import numpy as np
from origamy_sim import Mesh, Sheet, Simulation, Settings, Table, Crease, Material

def digest(a):
    return hashlib.sha256(np.ascontiguousarray(a, dtype=np.float64).tobytes()).hexdigest()[:12]

def run(name, settings, setup, steps=3):
    mesh = Mesh.grid(12, 12, 0.1, 0.1)
    sheet = Sheet(mesh, Material())
    sim = Simulation([sheet], settings)
    setup(sim, mesh)
    out = []
    for _ in range(steps):
        r = sim.step()
        out.append(f"{r.iterations}:{r.energy.total!r}")
    print(name, digest(sim.positions(0)), " ".join(out), flush=True)

def hang(sim, mesh):
    sim.hold(0, [0, 12])

def fold(sim, mesh):
    rest = mesh.rest
    sim.hold(0, np.flatnonzero(rest[:, 0] < 0.05))
    for u, v, _, _ in mesh.dihedrals.tolist():
        if abs(rest[u, 0] - 0.05) < 1e-9 and abs(rest[v, 0] - 0.05) < 1e-9:
            sim.set_crease(0, (u, v), Crease(stiffness=0.9, yield_moment=math.inf))
            sim.set_rest_angle(0, (u, v), 2.5)

def table(sim, mesh):
    pos = sim.positions(0)
    pos[:, 2] = 0.01
    sim.set_positions(0, pos)

run("hang", Settings(time_step=0.01), hang)
run("fold", Settings(time_step=math.inf), fold)
run("drop", Settings(time_step=0.01, table=Table()), table, steps=5)
