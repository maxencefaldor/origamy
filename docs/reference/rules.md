# Rules

Each rule names one fact a build can state about a step, and `origamy explain` prints the same condition as this page.
A refusal rule stops the action it states: the paper has no state after it, so the build stops at that script line.
A refusal rule is always an error, and the other rules state a fact about a step the paper takes (see [Feedback](../guide/feedback.md)).

| Rule | Level | Refuses the action | Fact |
| --- | --- | --- | --- |
| [`flap-off-paper`](#flap-off-paper) | `error` | yes | The flap point lies on no paper's inside. |
| [`flap-on-line`](#flap-on-line) | `error` | yes | The flap point lies on the fold line. |
| [`too-few-layers`](#too-few-layers) | `error` | yes | Fewer layers lie at the flap point than the action takes. |
| [`no-hinge`](#no-hinge) | `error` | yes | The paper to move is joined to no other paper along the fold line. |
| [`already-folded`](#already-folded) | `error` | yes | The fold line runs along a crease already folded the way it turns. |
| [`not-folded`](#not-folded) | `error` | yes | The unfold line runs along paper that is not folded there. |
| [`no-crease`](#no-crease) | `error` | yes | A collapse names a line with no crease of the paper along it. |
| [`no-spine`](#no-spine) | `error` | yes | The paper to reverse has no spine to turn inside out. |
| [`several-spines`](#several-spines) | `error` | yes | The paper to reverse could turn inside out about more than one spine. |
| [`no-side`](#no-side) | `error` | yes | The layer a petal or squash fold lifts has no folded side to open. |
| [`mountain-valley-count`](#mountain-valley-count) | `error` | yes | Mountains and valleys at a vertex do not differ by two. |
| [`unbalanced-angles`](#unbalanced-angles) | `error` | yes | The angles between folds at a vertex do not alternate to 180°. |
| [`layer-crossing`](#layer-crossing) | `error` | yes | No stacking of the faces keeps paper from passing through paper. |
| [`layer-order-unknown`](#layer-order-unknown) | `error` | yes | The layer solver stopped before finding a stacking. |
| [`not-flat`](#not-flat) | `error` | yes | The action folds paper that shaping has turned out of the plane. |
| [`hinge-bent`](#hinge-bent) | `error` | yes | The hinge of a shape step runs through paper turned apart before. |
| [`turns-through-paper`](#turns-through-paper) | `error` | yes | A shape step or a flex turns paper through paper. |
| [`no-flex`](#no-flex) | `error` | yes | No rigid motion turns the creases to the angles a flex names. |
| [`unreferenced-fold`](#unreferenced-fold) | `warn` | no | No point or line already on the paper locates the fold line. |

## flap-off-paper

The flap point lies on no paper's inside.

The point an action takes the paper at, or the point a collapse holds fixed, lies outside every face once folded, or on a face's edge.

## flap-on-line

The flap point lies on the fold line.

The point an action grabs the paper at lies on its fold line, so it names no side to move.

## too-few-layers

Fewer layers lie at the flap point than the action takes.

An action takes the top n layers at its flap point, and fewer than n faces cover that point once folded.

## no-hinge

The paper to move is joined to no other paper along the fold line.

The faces an action moves meet the faces that stay along no edge on the fold line; the action would turn that paper over, not fold it.

## already-folded

The fold line runs along a crease already folded the way it turns.

A fold turns paper about a crease on its line that is already folded that way: the moving face lies under the face that stays for a valley fold, or over it for a mountain fold.
A reverse fold turns no folded crease.

## not-folded

The unfold line runs along paper that is not folded there.

An unfold moves paper about an edge on its line whose two faces lie flat, on both sides of it, or about a line through the inside of a face.

## no-crease

A collapse names a line with no crease of the paper along it.

A collapse sets creases along a line or segment of the crease pattern, and no crease inside the paper lies along it; for a petal fold, no flat crease of the lifted layer lies along its line.
For a squash fold, no crease of the flap's top layer lies along its line, or no crease precreased in one of its layers runs from where its side meets the line.

## no-spine

The paper to reverse has no spine to turn inside out.

The tip a reverse fold turns splits into no upper and lower half, each one piece of paper, that lie one above the other and join only by folds.

## several-spines

The paper to reverse could turn inside out about more than one spine.

The tip a reverse fold turns splits into an upper and a lower half, each one piece of paper joined to the other only by folds, in several ways; no single way splits the layers at the flap point most evenly.

## no-side

The layer a petal or squash fold lifts has no folded side to open.

A petal fold lifts a layer whose creases along its line end at no folded side of the layer, so no side opens flat as the layer lifts.
A squash fold swings a top layer with no folded side off its line.

## mountain-valley-count

Mountains and valleys at a vertex do not differ by two.

At a vertex inside the paper with folds, the numbers of mountain and valley folds do not differ by exactly two (Maekawa's theorem).

## unbalanced-angles

The angles between folds at a vertex do not alternate to 180°.

At a vertex inside the paper, the reflections across its folds, taken in order around it, do not compose to the identity: the alternate angles between folds do not each sum to 180° (Kawasaki-Justin theorem).

## layer-crossing

No stacking of the faces keeps paper from passing through paper.

The layer solver proved that no order of the overlapping faces meets the taco-taco, taco-tortilla, tortilla-tortilla and transitivity constraints together with the stacking the action keeps.

## layer-order-unknown

The layer solver stopped before finding a stacking.

The layer solver reached its budget of dead ends before it found a stacking or proved that none exists.

## not-flat

The action folds paper that shaping has turned out of the plane.

A fold, unfold, precrease, collapse or named move runs after a shape step, a flex or a tilt, so the paper no longer lies flat to fold.

## hinge-bent

The hinge of a shape step runs through paper turned apart before.

The faces that stay along a shape step's line do not all lie where one rigid motion puts them, so the line is no straight hinge in space.

## turns-through-paper

A shape step or a flex turns paper through paper.

A face a shape step turns lies under a face that stays, for a turn up, or over it, for a turn down, where the two overlap in the same plane.
A flex folds a crease past flat, or passes two faces through each other.

## no-flex

No rigid motion turns the creases to the angles a flex names.

The solver that follows a flex finds no way to turn the other creases so that every face stays rigid and the paper stays whole around each vertex.

## unreferenced-fold

No point or line already on the paper locates the fold line.

The fold line passes through no two distinct points of the folded paper, puts no such point onto another, puts no line of the paper onto another, and is not perpendicular to such a line through such a point: the first four Huzita-Justin axioms do not build it from the paper's corners, creases and edges.
