# Joshua Tree Morphology Note

Target: Joshua tree (`Yucca brevifolia`)

## Crown Silhouette

- Open, sparse, and forked rather than leafy or conical.
- Mature plants should read as upright branching yucca forms with repeated terminal forks.
- Crown mass lives mostly at branch tips, with strong negative space between arms.

## Trunk And Structural Traits

- Main stem is stout, upright, and lightly tapering.
- Branches divide at terminal points instead of emerging continuously along the parent.
- Bark and leaf material detail remain metadata placeholders only; no texture or PBR asset pipeline work belongs here.

## Branching Habit

- Use the `dichotomous` generator family.
- Each branch level should fork into two main children at the parent tip.
- Rotate successive fork levels to avoid a flat, planar silhouette.

## Foliage

- Foliage should appear as terminal rosettes or spiky clusters.
- Until rosette-specific geometry exists, use endpoint foliage on the highest branch levels.
- `needle` polygon leaves are a temporary geometry stand-in for spiky yucca foliage.

## Scale Targets

- Height range for the prototype: 3m to 7m.
- Forked spread target: roughly 0.4x to 0.9x plant height.
- Trunk radius target: 0.2m to 0.6m.

## Tune First

- `trunk.height`
- `trunk.radius`
- `branches.level1.angle`
- `branches.level1.length`
- `branches.level2.rotation`
- `branches.level3.length`
- `leaves.count`
- `leaves.size`
