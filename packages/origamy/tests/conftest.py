# Copyright 2026 Maxence Faldor
"""Fixtures shared by the tests."""

import zlib
from collections.abc import Callable, Iterator
from pathlib import Path

import pytest

from origamy.field import tower

type Colors = Callable[[Path], set[tuple[int, int, int]]]


@pytest.fixture(autouse=True)
def own_tower() -> Iterator[None]:
    """Give each test its own tower of numbers, from the rationals, as a build has."""
    with tower(()):
        yield


@pytest.fixture
def colors() -> Colors:
    """A reader of the colors in a PNG file."""
    return png_colors


def png_colors(path: Path) -> set[tuple[int, int, int]]:
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
