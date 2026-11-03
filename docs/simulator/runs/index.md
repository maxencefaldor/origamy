# Runs

Each run below is the simulator at work on kami, recorded frame by frame: every frame is a solved physical state.
Drag to orbit, scroll to zoom, and slow time down to watch the paper move.
The figures under each run come from the step that frame shows.
The command `uv run python docs/simulator.py` records the runs, and the docs workflow records them before each build.

## A square folded over itself

A 15 cm square of kami lies on the table, with one crease along its diagonal.
A hand holds the lower right half, and the crease's rest angle turns from 0° to 179° over 1.5 s.
The time steps last 1/60 s.
The free half rises, turns over and lands on the held half, a paper thickness above it.

<iframe class="origamy-run" src="viewer.html?run=fold-over" title="A square folded over itself" loading="lazy"></iframe>

## Four sheets dropped on the table

Four 5 cm squares fall from 2 to 8 mm onto the table, each turned 15° from the one below.
The time steps last 1/120 s.
They settle into a stack, each layer a paper thickness above the one below, within the barrier's 2 µm gap.

<iframe class="origamy-run" src="viewer.html?run=stack" title="Four sheets dropped on the table" loading="lazy"></iframe>

## A strip swinging onto the table

A strip of 12 by 2 cm is clamped 6 cm over the table, along its first two columns, and let go at rest.
The strip swings down under its weight, and its end strikes the table, lifts once and comes to rest there.

<iframe class="origamy-run" src="viewer.html?run=strip" title="A strip swinging onto the table" loading="lazy"></iframe>

## Squares on two ramps

Two ramps of kami, 16 by 5 cm, are held white side up on the table at 15° and 30°.
A 3 cm square of kami lies at rest near the top of each, and the time steps last 1/240 s.
Friction between two sheets of kami, of coefficient 0.4, holds paper on slopes up to 21.8°.
The square on the gentler ramp stays, creeping by less than the 0.1 µm stick distance a step.
The square on the steeper ramp slides at the speed Coulomb's law gives, within 0.3%.
The square runs off the ramp onto the table, where friction stops it.

<iframe class="origamy-run" src="viewer.html?run=ramps" title="Squares on two ramps" loading="lazy"></iframe>

## Accordion pleats

A strip of 16 by 4 cm lies on the table with seven creases, valley and mountain in turn.
A hand holds its middle panel, and the creases' rest angles turn to ±150° in quasi-static steps.
The pleats rise off the table under gravity, and no panel passes through the table or another panel.

<iframe class="origamy-run" src="viewer.html?run=accordion" title="Accordion pleats" loading="lazy"></iframe>

## Miura-ori

An 8 by 6 Miura-ori of 15 mm parallelograms folds in the air, with gravity off.
Every crease's rest angle follows the rigid folding of the pattern, in quasi-static steps.
The sheet folds from flat to 132° without stretching, its folds within 2 × 10⁻⁷ rad of the rigid folding.

<iframe class="origamy-run" src="viewer.html?run=miura" title="Miura-ori" loading="lazy"></iframe>

## A fingertip folding a strip in half

A strip of kami 6 by 1.5 cm lies on the table, with a crease across its middle.
A hand holds its far edge down.
A fingertip of the finger pad's 14 mm radius pinches the near edge and carries it over onto the far edge.
The fingertip lets go, comes down beside the fold, and presses it with 2 N through a pad that yields at 700 N/m.
The fingertip lifts, the hand lets go, and the fold springs open to 35°, within the 30° to 40° that creased paper measures.
The time steps last 1/240 s.

<iframe class="origamy-run" src="viewer.html?run=fingertip" title="A fingertip folding a strip in half" loading="lazy"></iframe>
