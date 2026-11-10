"""Fold one test move with the thickness share, and print each stage: move.py name share."""
import os, sys, runpy
sys.path.insert(0, "packages/origamy/tests")
import importlib.util
spec = importlib.util.spec_from_file_location("tm", "packages/origamy/tests/test_moves.py")
tm = importlib.util.module_from_spec(spec); spec.loader.exec_module(tm)
from origamy.physics import Physics, simulate
from origamy_sim import Contact
name, share = sys.argv[1], float(sys.argv[2])
contact = Contact(thickness_share=share) if share else Contact(layers=False)
settings = Physics().settings.model_copy(update={"contact": contact})
if os.environ.get("TOL"):
    settings = settings.model_copy(update={"solver": settings.solver.model_copy(update={"tolerance": float(os.environ["TOL"])})})
f = simulate(tm.MOVES[name]().steps, Physics(settings=settings))
for s in f.stages:
    print(f"stage {s.step}: offset {s.offset*1e3:.2f} mm (creep {f.creep*1e3:.2f}), converged {s.converged}, {s.iterations} it, misordered {len(s.misordered)}")
