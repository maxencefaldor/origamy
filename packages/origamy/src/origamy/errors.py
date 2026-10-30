# Copyright 2026 Maxence Faldor
"""The errors that front ends show to users as messages instead of tracebacks."""


class OrigamyError(Exception):
    """A problem the caller can fix, such as a missing script or a refused fold."""
