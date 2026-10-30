# Copyright 2026 Maxence Faldor
"""A build written as a FOLD file, the format other origami tools read.

FOLD 1.2 (https://edemaine.github.io/fold/doc/spec.html) stores coordinates as floats.
The file holds the last crease pattern at the top level, then one folded form per step
in ``file_frames``. Exact coordinates ride along as ``origamy:vertices_exact``, as
`Number.data` writes them, with the radicands of their tower in ``origamy:tower``.
"""

from typing import Any

from origamy.result import BuildResult
from origamy.state import State

type FoldFile = dict[str, Any]


def to_fold(result: BuildResult, title: str) -> FoldFile:
    """The FOLD file of a build's steps."""
    states = [data.state.state() for data in result.steps]
    last = states[-1]
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
                "frame_title": data.description,
                "frame_classes": ["foldedForm"],
                "frame_attributes": ["2D"],
                **_folded(state),
            }
            for data, state in zip(result.steps, states, strict=True)
        ],
    }
    return document


def _pattern(state: State) -> FoldFile:
    return {
        "vertices_coords": [p.floats() for p in state.vertices],
        "origamy:vertices_exact": [[p.x.data(), p.y.data()] for p in state.vertices],
        **_topology(state),
    }


def _folded(state: State) -> FoldFile:
    places: list[tuple[float, float]] = [(0.0, 0.0)] * len(state.vertices)
    for face in state.faces:
        for vertex in face.corners:
            places[vertex] = face.place(state.vertices[vertex]).floats()
    flips = [state.flips(f) for f in range(len(state.faces))]
    return {
        "vertices_coords": places,
        **_topology(state),
        # +1 puts f above g along g's normal, which points down when g shows its back.
        "faceOrders": [
            [upper, lower, -1 if flips[lower] else 1]
            for upper, lower in sorted(state.above)
        ],
    }


def _topology(state: State) -> FoldFile:
    edges = sorted(state.edges)
    return {
        "edges_vertices": [list(edge) for edge in edges],
        "edges_assignment": [state.assignment(edge) for edge in edges],
        "faces_vertices": [list(face.corners) for face in state.faces],
    }
