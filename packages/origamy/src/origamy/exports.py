# Copyright 2026 Maxence Faldor
"""A build written as a FOLD file, the format other origami tools read.

FOLD 1.2 (https://edemaine.github.io/fold/doc/spec.html) stores coordinates as floats.
The file holds the last crease pattern at the top level, then one folded form per step
in ``file_frames``. Exact coordinates ride along as ``origamy:vertices_exact``, as
`Number.data` writes them, with the radicands of their tower in ``origamy:tower``.
Every crease has its fold angle, and a shaped step's folded form lies in 3D.
"""

from typing import Any

from origamy.paper import Step
from origamy.result import BuildResult
from origamy.space import fold_angles

type FoldFile = dict[str, Any]


def to_fold(result: BuildResult, title: str) -> FoldFile:
    """The FOLD file of a build's steps."""
    steps = [data.step() for data in result.steps]
    last = steps[-1]
    document: FoldFile = {
        "file_spec": 1.2,
        "file_creator": "origamy",
        "file_classes": ["singleModel"],
        "frame_title": title,
        "frame_classes": ["creasePattern"],
        "frame_attributes": ["2D"],
        "origamy:tower": list(result.tower),
        **_pattern(last),
        "file_frames": [
            {
                "frame_title": step.description,
                "frame_classes": ["foldedForm"],
                "frame_attributes": ["2D" if step.form is None else "3D"],
                **_folded(step),
            }
            for step in steps
        ],
    }
    return document


def _pattern(step: Step) -> FoldFile:
    state = step.state
    return {
        "vertices_coords": [p.floats() for p in state.vertices],
        "origamy:vertices_exact": [[p.x.data(), p.y.data()] for p in state.vertices],
        **_topology(step),
    }


def _folded(step: Step) -> FoldFile:
    state, form = step.state, step.form
    places: list[tuple[float, ...]] = [()] * len(state.vertices)
    for f, face in enumerate(state.faces):
        for vertex in face.corners:
            folded = state.folded_point(f, vertex)
            places[vertex] = folded.floats() if form is None else form.point(f, folded)
    flips = [state.flips(f) for f in range(len(state.faces))]
    return {
        "vertices_coords": places,
        **_topology(step),
        # +1 puts f above g along g's normal, which points down when g shows its back.
        "faceOrders": [
            [upper, lower, -1 if flips[lower] else 1]
            for upper, lower in sorted(state.above)
        ],
    }


def _topology(step: Step) -> FoldFile:
    state = step.state
    edges = sorted(state.edges)
    angles = fold_angles(state, step.form)
    return {
        "edges_vertices": [list(edge) for edge in edges],
        "edges_assignment": [state.assignment(edge) for edge in edges],
        "edges_foldAngle": [round(angles.get(edge, 0.0), 9) + 0.0 for edge in edges],
        "faces_vertices": [list(face.corners) for face in state.faces],
    }
