/**
 * Built-in species presets, bundled straight from the repository's
 * `presets/species/` TOML documents — the same files the CLI consumes.
 */

import birdOfParadiseToml from '../../../presets/species/bird_of_paradise.toml?raw';
import bistroBoxHedgeToml from '../../../presets/species/bistro_box_hedge.toml?raw';
import bistroCurvedHedgeToml from '../../../presets/species/bistro_curved_hedge.toml?raw';
import bistroTopiaryBallToml from '../../../presets/species/bistro_topiary_ball.toml?raw';
import englishIvyToml from '../../../presets/species/english_ivy.toml?raw';
import ginkgoToml from '../../../presets/species/ginkgo.toml?raw';
import italianCypressToml from '../../../presets/species/italian_cypress.toml?raw';
import japaneseMapleToml from '../../../presets/species/japanese_maple.toml?raw';
import joshuaPrototypeToml from '../../../presets/species/joshua_prototype.toml?raw';
import lindenToml from '../../../presets/species/linden.toml?raw';
import oakToml from '../../../presets/species/oak.toml?raw';
import palmToml from '../../../presets/species/palm.toml?raw';
import parisFlowerMoundToml from '../../../presets/species/paris_flower_mound.toml?raw';
import parisHangingCascadeToml from '../../../presets/species/paris_hanging_cascade.toml?raw';
import pineToml from '../../../presets/species/pine.toml?raw';
import topdownBroadleafToml from '../../../presets/species/topdown_broadleaf.toml?raw';
import topdownConiferToml from '../../../presets/species/topdown_conifer.toml?raw';
import topdownShrubToml from '../../../presets/species/topdown_shrub.toml?raw';
import willowToml from '../../../presets/species/willow.toml?raw';

export interface SpeciesPreset {
  id: string;
  label: string;
  toml: string;
  group?: 'trees' | 'bistro';
}

export const PRESETS: SpeciesPreset[] = [
  { id: 'oak', label: 'Oak', toml: oakToml, group: 'trees' },
  { id: 'linden', label: 'Parisian linden', toml: lindenToml, group: 'trees' },
  { id: 'japanese_maple', label: 'Japanese maple', toml: japaneseMapleToml, group: 'trees' },
  { id: 'ginkgo', label: 'Golden ginkgo', toml: ginkgoToml, group: 'trees' },
  { id: 'pine', label: 'Pine', toml: pineToml, group: 'trees' },
  { id: 'italian_cypress', label: 'Italian cypress', toml: italianCypressToml, group: 'trees' },
  { id: 'willow', label: 'Willow', toml: willowToml, group: 'trees' },
  { id: 'palm', label: 'Palm', toml: palmToml, group: 'trees' },
  { id: 'joshua_prototype', label: 'Joshua tree', toml: joshuaPrototypeToml, group: 'trees' },
  { id: 'topdown_broadleaf', label: 'Top-down broadleaf', toml: topdownBroadleafToml, group: 'trees' },
  { id: 'topdown_conifer', label: 'Top-down conifer', toml: topdownConiferToml, group: 'trees' },
  { id: 'topdown_shrub', label: 'Top-down shrub', toml: topdownShrubToml, group: 'trees' },
  { id: 'bistro_box_hedge', label: 'Boxwood café hedge', toml: bistroBoxHedgeToml, group: 'bistro' },
  { id: 'bistro_curved_hedge', label: 'Curved terrace hedge', toml: bistroCurvedHedgeToml, group: 'bistro' },
  { id: 'bistro_topiary_ball', label: 'Boxwood topiary globe', toml: bistroTopiaryBallToml, group: 'bistro' },
  { id: 'paris_flower_mound', label: 'Parisian pansy mound', toml: parisFlowerMoundToml, group: 'bistro' },
  { id: 'paris_hanging_cascade', label: 'Hanging flower cascade', toml: parisHangingCascadeToml, group: 'bistro' },
  { id: 'english_ivy', label: 'Climbing facade ivy', toml: englishIvyToml, group: 'bistro' },
  { id: 'bird_of_paradise', label: 'Bird of paradise', toml: birdOfParadiseToml, group: 'bistro' },
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
