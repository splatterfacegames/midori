/**
 * Built-in nature patch presets, bundled straight from the repository's
 * `presets/nature/` TOML documents — the same files the CLI consumes.
 */

import temperateForestFloorToml from '../../../presets/nature/temperate_forest_floor.toml?raw';
import floweringMeadowToml from '../../../presets/nature/flowering_meadow.toml?raw';
import aridScrubToml from '../../../presets/nature/arid_scrub.toml?raw';

export interface NaturePreset {
  id: string;
  label: string;
  toml: string;
}

export const NATURE_PRESETS: NaturePreset[] = [
  {
    id: 'temperate_forest_floor',
    label: 'Temperate Forest Floor',
    toml: temperateForestFloorToml,
  },
  { id: 'flowering_meadow', label: 'Flowering Meadow', toml: floweringMeadowToml },
  { id: 'arid_scrub', label: 'Arid Scrub', toml: aridScrubToml },
];
