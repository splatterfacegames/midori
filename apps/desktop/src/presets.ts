/**
 * Built-in species presets, bundled straight from the repository's
 * `presets/species/` TOML documents — the same files the CLI consumes.
 */

import oakToml from '../../../presets/species/oak.toml?raw';
import palmToml from '../../../presets/species/palm.toml?raw';
import pineToml from '../../../presets/species/pine.toml?raw';
import willowToml from '../../../presets/species/willow.toml?raw';

export interface SpeciesPreset {
  id: string;
  label: string;
  toml: string;
}

export const PRESETS: SpeciesPreset[] = [
  { id: 'oak', label: 'Oak', toml: oakToml },
  { id: 'pine', label: 'Pine', toml: pineToml },
  { id: 'palm', label: 'Palm', toml: palmToml },
  { id: 'willow', label: 'Willow', toml: willowToml },
];

/**
 * Built-in NaturePatch presets, bundled straight from the repository's
 * `presets/nature/` TOML documents — the same files `midori nature` consumes.
 */

import aridScrubToml from '../../../presets/nature/arid_scrub.toml?raw';
import floweringMeadowToml from '../../../presets/nature/flowering_meadow.toml?raw';
import temperateForestFloorToml from '../../../presets/nature/temperate_forest_floor.toml?raw';

export const NATURE_PRESETS: SpeciesPreset[] = [
  { id: 'arid_scrub', label: 'Arid scrub', toml: aridScrubToml },
  { id: 'flowering_meadow', label: 'Flowering meadow', toml: floweringMeadowToml },
  { id: 'temperate_forest_floor', label: 'Temperate forest floor', toml: temperateForestFloorToml },
];
