# Oak Morphology Note

Target: English oak / pedunculate oak (`Quercus robur`)

## Crown Silhouette

- Broad, rounded, and irregular rather than conical.
- Mature trees should read wider than they are tall when open-grown.
- Crown mass starts low enough to show heavy primary limbs but should leave the trunk visible.

## Trunk And Structural Traits

- Thick trunk with strong taper and a broad base.
- Primary limbs should feel heavy, crooked, and unevenly spaced.
- Bark/material detail is represented by metadata placeholders only in this goal; no texture or PBR asset pipeline work belongs here.

## Branching Habit

- Use the `weber_penn` family for the current preset.
- Level 1 branches should emerge at wide angles, with negative gravity values helping them sag or spread.
- Secondary and tertiary branches should become more irregular and more upward/noisy toward the outer crown.

## Foliage

- Leaves are clustered on outer branch levels.
- `polygon` oak-lobed leaves are preferred for close LODs.
- Far LODs may use geometry-only crown cards until a later parked texture/impostor pipeline exists.

## Scale Targets

- Height range for game-ready presets: 6m to 18m.
- Spread target: roughly 0.9x to 1.4x tree height depending on age and scene use.
- Trunk radius target: 0.35m to 1.0m for mature specimens.

## Tune First

- `trunk.height`
- `trunk.radius`
- `crown.width_ratio`
- `crown.offset`
- `branches.level1.angle`
- `branches.level1.gravity`
- `branches.level2.length`
- `leaves.count`
