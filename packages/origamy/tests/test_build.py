# Copyright 2026 Maxence Faldor
import asyncio
import json
import shutil
import zlib
from pathlib import Path

import pytest

from origamy import Line, Paper, RefusedError, fold_onto, service
from origamy.layers import NoStackingError, Problem, solve
from origamy.state import State

EXAMPLES = Path(__file__).parents[3] / "examples"

REFUSED = """\
from origamy import Line, Paper


def fold():
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    paper.valley(Line.through((2, 0), (2, 1)), flap=("1/4", "1/2"))
    return paper
"""


def build(script: Path, *, pictures: bool = False) -> service.BuildReport:
    request = service.BuildRequest(script=script, pictures=pictures)
    return asyncio.run(service.build(request))


def test_the_paper_records_each_step_and_its_line() -> None:
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    paper.turn_over()
    assert [step.action for step in paper.steps] == ["start", "valley", "turn-over"]
    assert paper.steps[1].source is not None
    assert paper.steps[1].source.site.function == (
        "test_the_paper_records_each_step_and_its_line"
    )
    assert paper.where((1, 1)) == paper.where((0, 1))


def test_a_refused_action_keeps_the_steps_before_it() -> None:
    paper = Paper.square()
    with pytest.raises(RefusedError) as refused:
        paper.valley(Line.through((2, 0), (2, 1)), flap=("1/2", "1/2"))
    assert refused.value.paper is paper
    assert refused.value.refused.diagnostics[0].rule == "no-hinge"
    assert len(paper.steps) == 1


def test_an_unreferenced_fold_is_a_warning() -> None:
    paper = Paper.square()
    paper.valley(Line.through(("3/10", 0), ("3/10", 1)), flap=("1/10", "1/2"))
    assert [d.rule for d in paper.steps[-1].diagnostics] == ["unreferenced-fold"]
    paper.valley(fold_onto((1, 0), ("3/10", 0)), flap=("9/10", "1/2"))
    assert not paper.steps[-1].diagnostics


def test_build_reports_steps_and_writes_a_fold_file(tmp_path: Path) -> None:
    script = Path(shutil.copy(EXAMPLES / "cup.py", tmp_path))
    report = build(script)
    assert report.ok, report.text
    assert report.steps == 9
    assert report.text.startswith("cup.py: 9 steps folded")
    document = json.loads((tmp_path / "cup.fold").read_text(encoding="utf-8"))
    assert document["file_spec"] == 1.2
    assert len(document["file_frames"]) == 10
    assert set(document["edges_assignment"]) <= {"M", "V", "F", "B"}


def test_build_reports_a_refusal_at_its_line(tmp_path: Path) -> None:
    script = tmp_path / "broken.py"
    script.write_text(REFUSED, encoding="utf-8")
    report = build(script)
    assert not report.ok
    assert report.refused is not None
    assert report.refused.step == 2
    assert report.refused.source is not None
    assert report.refused.source.site.line == 7
    assert "error[no-hinge] step 2" in report.text


def test_build_reports_a_failing_script(tmp_path: Path) -> None:
    script = tmp_path / "failing.py"
    script.write_text("def fold():\n    return 1 / 0\n", encoding="utf-8")
    report = build(script)
    assert report.failure is not None
    assert report.failure.error == "ZeroDivisionError"
    assert report.failure.frames[0].line == 2


def test_explain_lists_every_rule() -> None:
    text = service.explain(service.ExplainRequest())
    assert "layer-crossing (error, refuses the action)" in text
    assert "unreferenced-fold (warn)" in text


def test_the_solver_names_the_constraints_that_conflict() -> None:
    paper = Paper.square()
    paper.valley(Line.through(("1/2", 0), ("1/2", 1)), flap=("3/4", "1/2"))
    paper.valley(Line.through((0, "1/2"), (1, "1/2")), flap=("1/4", "3/4"))
    state = paper.state
    upper, lower = next(iter(state.above))
    with pytest.raises(NoStackingError) as error:
        solve(Problem(state), [(upper, lower), (lower, upper)])
    assert error.value.fixed
    folded_in_four = State(state.vertices, state.faces)
    stacks = solve(Problem(folded_in_four), sorted(state.above))
    assert stacks == state.above


def test_pictures_have_the_paper_in_them(tmp_path: Path) -> None:
    script = Path(shutil.copy(EXAMPLES / "cup.py", tmp_path))
    report = build(script, pictures=True)
    assert report.ok, report.text
    assert (200, 85, 61) in colors(tmp_path / "cup.png")  # the front, folded
    assert (200, 85, 61) in colors(tmp_path / "cup.steps.png")


def colors(path: Path) -> set[tuple[int, int, int]]:
    """The colors of an 8-bit RGB or RGBA PNG, decoded with the standard library."""
    data, chunks, position = path.read_bytes(), {}, 8
    while position < len(data):
        length = int.from_bytes(data[position : position + 4])
        kind = data[position + 4 : position + 8]
        chunks[kind] = (
            chunks.get(kind, b"") + data[position + 8 : position + 8 + length]
        )
        position += 12 + length
    header = chunks[b"IHDR"]
    width, height = int.from_bytes(header[:4]), int.from_bytes(header[4:8])
    channels = {2: 3, 6: 4}[header[9]]
    raw, stride = zlib.decompress(chunks[b"IDAT"]), width * channels
    previous, found = bytearray(stride), set[tuple[int, int, int]]()
    for row in range(height):
        start = row * (stride + 1)
        kind, line = raw[start], bytearray(raw[start + 1 : start + 1 + stride])
        for i in range(stride):
            a = line[i - channels] if i >= channels else 0
            b, c = previous[i], previous[i - channels] if i >= channels else 0
            line[i] = (line[i] + (0, a, b, (a + b) // 2, _paeth(a, b, c))[kind]) & 255
        found.update(
            (line[i], line[i + 1], line[i + 2]) for i in range(0, stride, channels)
        )
        previous = line
    return found


def _paeth(a: int, b: int, c: int) -> int:
    guess = a + b - c
    return min((a, b, c), key=lambda value: abs(guess - value))
