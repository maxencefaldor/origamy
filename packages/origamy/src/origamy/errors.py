# Copyright 2026 Maxence Faldor
"""The errors that front ends show to users as messages instead of tracebacks."""


class OrigamyError(Exception):
    """A problem the caller can fix, such as a missing script or a refused fold."""


class OutsideFieldError(OrigamyError, ArithmeticError):
    """A number that is not of the form a + b√2, such as √3 or the cube root of 2."""
