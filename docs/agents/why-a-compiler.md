# Why a compiler

The page explains why origamy works like a compiler, and what it leaves to the agent.

## Feedback is facts

Agents write working code because a compiler answers every attempt with exact, local feedback.
An error names a rule, a place and a line of source, so the agent knows what to edit and when it is done.
A photo of folded paper shows only that something looks wrong, and leaves the agent to find the fold behind it.
A build gives a fold script the same loop: each refusal names its rule, the faces involved and the script line behind them.

```text
error[no-hinge] step 2, valley fold along x = 2, the paper at (1/4, 1/2): broken.py:7 in fold(): the paper to move meets no other paper along x = 2
  faces: 0, 1
```

A fact says what is true about an action, and never how to fix it.
Deciding what to change stays with the agent, and advice belongs in a skill, such as the [fold-model skill](connect.md#skill).

## The stages of a build

A build runs the stages of a compiler, from a source file to the files people use:

| Compiler | origamy |
| --- | --- |
| Source file | Fold script: a Python file whose `fold()` returns a `Paper` |
| Parsing | Running the script in its own process, which records the script lines behind each action |
| Intermediate representation | The state of the paper: its crease pattern, each face's place and the stacking, in exact numbers |
| Passes | The checks of each action: its flap point, its hinge, the vertices of a collapse and the layer solver |
| Diagnostics | Facts: a rule, the step, the script line and the faces involved |
| Code generation | The pictures, the 3D viewer and the FOLD file |

## No legal moves

The build never lists the actions that would work.
The agent proposes any action, and the build says what happens: the paper takes it, or refuses it with the facts of why.
A fold line can be any line through the paper, so the possible folds form no list to choose from.
The [skill](connect.md#skill) teaches the actions and the constructions that find fold lines, and the agent decides where to fold.

## Mechanisms, not designs

The API offers exact geometry and folding actions, never a design generator.
The [crane](https://github.com/maxencefaldor/origamy/blob/main/examples/crane.py) is a script that folds a bird base, narrows its legs, reverse folds them up, and folds the wings.
Named moves such as reverse and petal folds compute only their own creases, judged like any collapse.
The design stays the agent's work, and the build checks every step of it.

## Why exactness matters

Origami geometry runs on square roots: the 22.5° creases of the bird base take √2.
Bisecting such an angle again nests one root in another, and the crane's tower holds √2 and √(4 - 2√2).
Floats would round every fold a little, and a corner meant to land on a crease would miss it by a rounding error.
Exact numbers give every check one answer: a point lies on a line, or it lies off it.
Float estimates with error bounds still decide most signs fast, and exact arithmetic decides only what they leave open.
The first five Huzita-Justin axioms build lines with arithmetic and square roots alone, so every construction stays exact.

## Where the comparison stops

A build in which the paper takes every action has a valid stacking at every step: no paper passes through paper.
The report cannot say whether the paper looks like the intended model.
The pictures answer that question, so the loop ends when the paper takes every action and the pictures show the model.
