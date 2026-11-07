# Copyright 2026 Maxence Faldor
"""The rules: every fact a build can state, by name, with its level and exact condition.

An action that breaks a refusal rule cannot happen: the paper has no state after it, so
the build stops there and the rule is always an error. Every other rule states a fact
about a step the paper can take, at a level of ``error``, ``warn`` or ``ignore``.
"""

from dataclasses import dataclass
from enum import StrEnum

from origamy.errors import OrigamyError
from origamy.state import State


class Level(StrEnum):
    """How a build reports a rule's facts."""

    ERROR = "error"
    WARN = "warn"
    IGNORE = "ignore"


@dataclass(frozen=True, slots=True)
class Rule:
    """A fact a build can state."""

    name: str
    level: Level
    summary: str
    condition: str
    refusal: bool = False
    """Whether the fact stops an action: the paper has no state after it."""


RULES = {
    rule.name: rule
    for rule in (
        Rule(
            "flap-off-paper",
            Level.ERROR,
            "The flap point lies on no paper's inside.",
            "The point an action takes the paper at, or the point a collapse holds "
            "fixed, lies outside every face once folded, or on a face's edge.",
            refusal=True,
        ),
        Rule(
            "flap-on-line",
            Level.ERROR,
            "The flap point lies on the fold line.",
            "The point an action grabs the paper at lies on its fold line, so it names "
            "no side to move.",
            refusal=True,
        ),
        Rule(
            "too-few-layers",
            Level.ERROR,
            "Fewer layers lie at the flap point than the action takes.",
            "An action takes the top n layers at its flap point, and fewer than n "
            "faces cover that point once folded.",
            refusal=True,
        ),
        Rule(
            "no-hinge",
            Level.ERROR,
            "The paper to move is joined to no other paper along the fold line.",
            "The faces an action moves meet the faces that stay along no edge on the "
            "fold line; the action would turn that paper over, not fold it.",
            refusal=True,
        ),
        Rule(
            "already-folded",
            Level.ERROR,
            "The fold line runs along a crease already folded the way it turns.",
            "A fold turns paper about a crease on its line that is already folded that "
            "way: the moving face lies under the face that stays for a valley fold, or "
            "over it for a mountain fold. A reverse fold turns no folded crease.",
            refusal=True,
        ),
        Rule(
            "not-folded",
            Level.ERROR,
            "The unfold line runs along paper that is not folded there.",
            "An unfold moves paper about an edge on its line whose two faces lie flat, "
            "on both sides of it, or about a line through the inside of a face.",
            refusal=True,
        ),
        Rule(
            "no-crease",
            Level.ERROR,
            "A collapse names a line with no crease of the paper along it.",
            "A collapse sets creases along a line or segment of the crease pattern, "
            "and no crease inside the paper lies along it; for a petal fold, no flat "
            "crease of the lifted layer lies along its line. For a squash fold, no "
            "crease of the flap's top layer lies along its line, or no crease "
            "precreased in one of its layers runs from where its side meets the line.",
            refusal=True,
        ),
        Rule(
            "no-spine",
            Level.ERROR,
            "The paper to reverse has no spine to turn inside out.",
            "The tip a reverse fold turns splits into no upper and lower half, each "
            "one piece of paper, that lie one above the other and join only by folds.",
            refusal=True,
        ),
        Rule(
            "several-spines",
            Level.ERROR,
            "The paper to reverse could turn inside out about more than one spine.",
            "The tip a reverse fold turns splits into an upper and a lower half, each "
            "one piece of paper joined to the other only by folds, in several ways; "
            "no single way splits the layers at the flap point most evenly.",
            refusal=True,
        ),
        Rule(
            "no-side",
            Level.ERROR,
            "The layer a petal or squash fold lifts has no folded side to open.",
            "A petal fold lifts a layer whose creases along its line end at no "
            "folded side of the layer, so no side opens flat as the layer lifts. "
            "A squash fold swings a top layer with no folded side off its line.",
            refusal=True,
        ),
        Rule(
            "paper-folded",
            Level.ERROR,
            "A crease pattern creases paper that lies folded.",
            "Creasing along segments of the crease pattern needs the paper open in "
            "one layer, and some faces lie in different places.",
            refusal=True,
        ),
        Rule(
            "crease-off-paper",
            Level.ERROR,
            "A crease of a crease pattern runs off the paper.",
            "An end of a segment to crease lies outside the open paper.",
            refusal=True,
        ),
        Rule(
            "loose-crease",
            Level.ERROR,
            "A crease ends inside a face, or creases leave a face that is not convex.",
            "The segments to crease, the paper's edges and its creases leave a crease "
            "with the same face on both sides, or a face that is not convex.",
            refusal=True,
        ),
        Rule(
            "mountain-valley-count",
            Level.ERROR,
            "Mountains and valleys at a vertex do not differ by two.",
            "At a vertex inside the paper with folds, the numbers of mountain and "
            "valley folds do not differ by exactly two (Maekawa's theorem).",
            refusal=True,
        ),
        Rule(
            "unbalanced-angles",
            Level.ERROR,
            "The angles between folds at a vertex do not alternate to 180°.",
            "At a vertex inside the paper, the reflections across its folds, taken "
            "in order around it, do not compose to the identity: the alternate "
            "angles between folds do not each sum to 180° (Kawasaki-Justin theorem).",
            refusal=True,
        ),
        Rule(
            "layer-crossing",
            Level.ERROR,
            "No stacking of the faces keeps paper from passing through paper.",
            "The layer solver proved that no order of the overlapping faces meets the "
            "taco-taco, taco-tortilla, tortilla-tortilla and transitivity constraints "
            "together with the stacking the action keeps.",
            refusal=True,
        ),
        Rule(
            "layer-order-unknown",
            Level.ERROR,
            "The layer solver stopped before finding a stacking.",
            "The layer solver reached its budget of dead ends before it found a "
            "stacking or proved that none exists.",
            refusal=True,
        ),
        Rule(
            "settled",
            Level.ERROR,
            "The action follows a settle step.",
            "A settle step lets the simulator bend the paper, which no exact state "
            "holds, so it ends the build.",
            refusal=True,
        ),
        Rule(
            "not-flat",
            Level.ERROR,
            "The action folds paper that shaping has turned out of the plane.",
            "A fold, unfold, precrease, collapse or named move runs after a shape "
            "step, a flex or a tilt, so the paper no longer lies flat to fold.",
            refusal=True,
        ),
        Rule(
            "hinge-bent",
            Level.ERROR,
            "The hinge of a shape step runs through paper turned apart before.",
            "The faces that stay along a shape step's line do not all lie where one "
            "rigid motion puts them, so the line is no straight hinge in space.",
            refusal=True,
        ),
        Rule(
            "turns-through-paper",
            Level.ERROR,
            "A shape step or a flex turns paper through paper.",
            "A face a shape step turns lies under a face that stays, for a turn up, or "
            "over it, for a turn down, where the two overlap in the same plane. A flex "
            "folds a crease past flat, or passes two faces through each other.",
            refusal=True,
        ),
        Rule(
            "no-flex",
            Level.ERROR,
            "No rigid motion turns the creases to the angles a flex names.",
            "The solver that follows a flex finds no way to turn the other creases so "
            "that every face stays rigid and the paper stays whole around each vertex.",
            refusal=True,
        ),
        Rule(
            "unreferenced-fold",
            Level.WARN,
            "No point or line already on the paper locates the fold line.",
            "The fold line passes through no two distinct points of the folded paper, "
            "puts no such point onto another, puts no line of the paper onto another, "
            "and is not perpendicular to such a line through such a point: the first "
            "four Huzita-Justin axioms do not build it from the paper's corners, "
            "creases and edges.",
        ),
    )
}


@dataclass(frozen=True, slots=True)
class Diagnostic:
    """One statement of a rule's fact about a step."""

    rule: str
    level: Level
    message: str
    faces: tuple[int, ...] = ()
    """The faces the fact is about, as ids in the state of the step that states it."""

    @classmethod
    def of(cls, name: str, message: str, faces: tuple[int, ...] = ()) -> Diagnostic:
        """A diagnostic of a rule, at the rule's level."""
        return cls(name, RULES[name].level, message, faces)


class RefusalError(OrigamyError):
    """An action the paper cannot take, with the facts of why."""

    def __init__(
        self, message: str, *, rule: str, state: State, faces: tuple[int, ...] = ()
    ) -> None:
        """State the fact, name its refusal rule, and list the faces it is about.

        ``state`` is the paper the faces' ids name: the paper before the action, or as
        the action cut it along its line before the refusal.
        """
        super().__init__(message)
        self.diagnostic = Diagnostic.of(rule, message, faces)
        self.state = state


def rule(name: str) -> Rule:
    """The rule of a name, or an error that lists every rule."""
    if name not in RULES:
        msg = f"no rule is named {name!r}; the rules are {', '.join(RULES)}"
        raise OrigamyError(msg)
    return RULES[name]
