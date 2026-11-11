"""one.py script: fold one example with thickness; print each stage as it ends, then a summary."""
import asyncio, sys, time
from pathlib import Path
from origamy import Number
from origamy.field import tower
from origamy import physics as ph
from origamy.physics import Physics, simulate
from origamy.service import run_script
t0 = time.perf_counter()
for name in ("fold", "settle"):
    orig = getattr(ph._Folder, name)
    def wrap(self, *a, _orig=orig):
        s = _orig(self, *a)
        print(f"  stage {s.step}: offset {s.offset*1e3:.2f} mm, converged {s.converged}, {s.iterations} it, misordered {len(s.misordered)}, crossing {s.crossing*1e3:.3f} mm, height {s.frames[-1][:,2].max()*1e3:.2f} mm{', settled' if s.settled else ''}, at {time.perf_counter()-t0:.0f}s", flush=True)
        return s
    setattr(ph._Folder, name, wrap)
r = asyncio.run(run_script(Path(sys.argv[1]), timeout=900))
with tower(Number.from_data(x) for x in r.tower):
    fo = simulate([d.step() for d in r.steps], Physics())
st = fo.stages
print(f"SUMMARY {Path(sys.argv[1]).stem}: {len(st)} stages, {len(fo.mesh.triangles)} triangles, max offset {max(s.offset for s in st)*1e3:.2f} mm, unconverged {sum(not s.converged for s in st)}, misordered {sum(bool(s.misordered) for s in st)}, crossing {max(s.crossing for s in st)*1e3:.3f} mm, {time.perf_counter()-t0:.0f}s", flush=True)
