/**
 * Built-in species presets, bundled straight from the repository's
 * `presets/species/` TOML documents — the same files the CLI consumes.
 */

import ginkgoToml from '../../../presets/species/ginkgo.toml?raw';
import japaneseMapleToml from '../../../presets/species/japanese_maple.toml?raw';
import joshuaPrototypeToml from '../../../presets/species/joshua_prototype.toml?raw';
import oakToml from '../../../presets/species/oak.toml?raw';
import palmToml from '../../../presets/species/palm.toml?raw';
import pineToml from '../../../presets/species/pine.toml?raw';
import topdownBroadleafToml from '../../../presets/species/topdown_broadleaf.toml?raw';
import topdownConiferToml from '../../../presets/species/topdown_conifer.toml?raw';
import topdownShrubToml from '../../../presets/species/topdown_shrub.toml?raw';
import willowToml from '../../../presets/species/willow.toml?raw';

export interface SpeciesPreset {
  id: string;
  label: string;
  toml: string;
}

export const PRESETS: SpeciesPreset[] = [
  { id: 'oak', label: 'Oak', toml: oakToml },
  { id: 'japanese_maple', label: 'Japanese maple', toml: japaneseMapleToml },
  { id: 'ginkgo', label: 'Golden ginkgo', toml: ginkgoToml },
  { id: 'pine', label: 'Pine', toml: pineToml },
  { id: 'willow', label: 'Willow', toml: willowToml },
  { id: 'palm', label: 'Palm', toml: palmToml },
  { id: 'joshua_prototype', label: 'Joshua tree', toml: joshuaPrototypeToml },
  { id: 'topdown_broadleaf', label: 'Top-down broadleaf', toml: topdownBroadleafToml },
  { id: 'topdown_conifer', label: 'Top-down conifer', toml: topdownConiferToml },
  { id: 'topdown_shrub', label: 'Top-down shrub', toml: topdownShrubToml },
];

/**
 * Built-in NaturePatch presets, bundled straight from the repository's
 * `presets/nature/` TOML documents — the same files `midori nature` consumes.
 */

import alpineMeadowToml from '../../../presets/nature/alpine_meadow.toml?raw';
import aridScrubToml from '../../../presets/nature/arid_scrub.toml?raw';
import floweringMeadowToml from '../../../presets/nature/flowering_meadow.toml?raw';
import pampasMeadowToml from '../../../presets/nature/pampas_meadow.toml?raw';
import temperateForestFloorToml from '../../../presets/nature/temperate_forest_floor.toml?raw';

export const NATURE_PRESETS: SpeciesPreset[] = [
  { id: 'pampas_meadow', label: 'Susuki pampas field', toml: pampasMeadowToml },
  { id: 'alpine_meadow', label: 'Alpine meadow', toml: alpineMeadowToml },
  { id: 'flowering_meadow', label: 'Flowering meadow', toml: floweringMeadowToml },
  { id: 'temperate_forest_floor', label: 'Temperate forest floor', toml: temperateForestFloorToml },
  { id: 'arid_scrub', label: 'Arid scrub', toml: aridScrubToml },
];
