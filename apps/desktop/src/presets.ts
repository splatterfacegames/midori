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
import perthCanaryIslandDatePalmToml from '../../../presets/species/perth_canary_island_date_palm.toml?raw';
import perthCapeLilacToml from '../../../presets/species/perth_cape_lilac.toml?raw';
import perthJacarandaToml from '../../../presets/species/perth_jacaranda.toml?raw';
import perthLemonScentedGumToml from '../../../presets/species/perth_lemon_scented_gum.toml?raw';
import perthLondonPlaneToml from '../../../presets/species/perth_london_plane.toml?raw';
import perthMoretonBayFigToml from '../../../presets/species/perth_moreton_bay_fig.toml?raw';
import perthNorfolkIslandPineToml from '../../../presets/species/perth_norfolk_island_pine.toml?raw';
import perthOliveToml from '../../../presets/species/perth_olive.toml?raw';
import perthPeppercornToml from '../../../presets/species/perth_peppercorn.toml?raw';
import perthPortJacksonFigToml from '../../../presets/species/perth_port_jackson_fig.toml?raw';
import perthQueenslandBoxToml from '../../../presets/species/perth_queensland_box.toml?raw';
import perthRiverRedGumToml from '../../../presets/species/perth_river_red_gum.toml?raw';
import pineToml from '../../../presets/species/pine.toml?raw';
import topdownBroadleafToml from '../../../presets/species/topdown_broadleaf.toml?raw';
import topdownConiferToml from '../../../presets/species/topdown_conifer.toml?raw';
import topdownShrubToml from '../../../presets/species/topdown_shrub.toml?raw';
import waBullBanksiaToml from '../../../presets/species/wa_bull_banksia.toml?raw';
import waChristmasTreeToml from '../../../presets/species/wa_christmas_tree.toml?raw';
import waCoojongToml from '../../../presets/species/wa_coojong.toml?raw';
import waFirewoodBanksiaToml from '../../../presets/species/wa_firewood_banksia.toml?raw';
import waFloodedGumToml from '../../../presets/species/wa_flooded_gum.toml?raw';
import waGrassTreeToml from '../../../presets/species/wa_grass_tree.toml?raw';
import waHarshHakeaToml from '../../../presets/species/wa_harsh_hakea.toml?raw';
import waJarrahToml from '../../../presets/species/wa_jarrah.toml?raw';
import waJointedRushToml from '../../../presets/species/wa_jointed_rush.toml?raw';
import waKangarooPawToml from '../../../presets/species/wa_kangaroo_paw.toml?raw';
import waKarriToml from '../../../presets/species/wa_karri.toml?raw';
import waMarriToml from '../../../presets/species/wa_marri.toml?raw';
import waModongToml from '../../../presets/species/wa_modong.toml?raw';
import waOleariaToml from '../../../presets/species/wa_olearia.toml?raw';
import waPeppermintToml from '../../../presets/species/wa_peppermint.toml?raw';
import waRedTingleToml from '../../../presets/species/wa_red_tingle.toml?raw';
import waSaltwaterPaperbarkToml from '../../../presets/species/wa_saltwater_paperbark.toml?raw';
import waScaevolaToml from '../../../presets/species/wa_scaevola.toml?raw';
import waSheoakToml from '../../../presets/species/wa_sheoak.toml?raw';
import waSlenderBanksiaToml from '../../../presets/species/wa_slender_banksia.toml?raw';
import waSpinifexToml from '../../../presets/species/wa_spinifex.toml?raw';
import waStinkwoodToml from '../../../presets/species/wa_stinkwood.toml?raw';
import waSwampBanksiaToml from '../../../presets/species/wa_swamp_banksia.toml?raw';
import waSwampPaperbarkToml from '../../../presets/species/wa_swamp_paperbark.toml?raw';
import waSwampSheoakToml from '../../../presets/species/wa_swamp_sheoak.toml?raw';
import waSwordSedgeToml from '../../../presets/species/wa_sword_sedge.toml?raw';
import waTuartToml from '../../../presets/species/wa_tuart.toml?raw';
import waWandooToml from '../../../presets/species/wa_wandoo.toml?raw';
import waZamiaToml from '../../../presets/species/wa_zamia.toml?raw';
import willowToml from '../../../presets/species/willow.toml?raw';

export interface SpeciesPreset {
  id: string;
  label: string;
  toml: string;
  /**
   * Library section. `wa_native` is the native flora of south-west Western
   * Australia; `perth_planted` the planted, non-native trees of Perth's
   * streets and parks, kept apart so the two are never confused.
   */
  group?: 'trees' | 'bistro' | 'wa_native' | 'perth_planted';
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
  { id: 'perth_canary_island_date_palm', label: 'Canary Island date palm', toml: perthCanaryIslandDatePalmToml, group: 'perth_planted' },
  { id: 'perth_cape_lilac', label: 'Cape lilac / white cedar', toml: perthCapeLilacToml, group: 'perth_planted' },
  { id: 'perth_jacaranda', label: 'Jacaranda', toml: perthJacarandaToml, group: 'perth_planted' },
  { id: 'perth_lemon_scented_gum', label: 'Lemon-scented gum', toml: perthLemonScentedGumToml, group: 'perth_planted' },
  { id: 'perth_london_plane', label: 'London plane', toml: perthLondonPlaneToml, group: 'perth_planted' },
  { id: 'perth_moreton_bay_fig', label: 'Moreton Bay fig', toml: perthMoretonBayFigToml, group: 'perth_planted' },
  { id: 'perth_norfolk_island_pine', label: 'Norfolk Island pine', toml: perthNorfolkIslandPineToml, group: 'perth_planted' },
  { id: 'perth_olive', label: 'Olive', toml: perthOliveToml, group: 'perth_planted' },
  { id: 'perth_peppercorn', label: 'Peppercorn tree', toml: perthPeppercornToml, group: 'perth_planted' },
  { id: 'perth_port_jackson_fig', label: 'Port Jackson fig', toml: perthPortJacksonFigToml, group: 'perth_planted' },
  { id: 'perth_queensland_box', label: 'Queensland box / brush box', toml: perthQueenslandBoxToml, group: 'perth_planted' },
  { id: 'perth_river_red_gum', label: 'River red gum', toml: perthRiverRedGumToml, group: 'perth_planted' },
  { id: 'wa_bull_banksia', label: 'Bull banksia', toml: waBullBanksiaToml, group: 'wa_native' },
  { id: 'wa_christmas_tree', label: 'WA Christmas tree / moodjar', toml: waChristmasTreeToml, group: 'wa_native' },
  { id: 'wa_coojong', label: 'Coojong / orange wattle', toml: waCoojongToml, group: 'wa_native' },
  { id: 'wa_firewood_banksia', label: 'Firewood banksia', toml: waFirewoodBanksiaToml, group: 'wa_native' },
  { id: 'wa_flooded_gum', label: 'Flooded gum', toml: waFloodedGumToml, group: 'wa_native' },
  { id: 'wa_grass_tree', label: 'Grass tree / balga', toml: waGrassTreeToml, group: 'wa_native' },
  { id: 'wa_harsh_hakea', label: 'Harsh hakea', toml: waHarshHakeaToml, group: 'wa_native' },
  { id: 'wa_jarrah', label: 'Jarrah', toml: waJarrahToml, group: 'wa_native' },
  { id: 'wa_jointed_rush', label: 'Jointed rush', toml: waJointedRushToml, group: 'wa_native' },
  { id: 'wa_kangaroo_paw', label: 'Red and green kangaroo paw', toml: waKangarooPawToml, group: 'wa_native' },
  { id: 'wa_karri', label: 'Karri', toml: waKarriToml, group: 'wa_native' },
  { id: 'wa_marri', label: 'Marri', toml: waMarriToml, group: 'wa_native' },
  { id: 'wa_modong', label: 'Modong / stout paperbark', toml: waModongToml, group: 'wa_native' },
  { id: 'wa_olearia', label: 'Coastal daisybush', toml: waOleariaToml, group: 'wa_native' },
  { id: 'wa_peppermint', label: 'WA peppermint', toml: waPeppermintToml, group: 'wa_native' },
  { id: 'wa_red_tingle', label: 'Red tingle', toml: waRedTingleToml, group: 'wa_native' },
  { id: 'wa_saltwater_paperbark', label: 'Saltwater paperbark', toml: waSaltwaterPaperbarkToml, group: 'wa_native' },
  { id: 'wa_scaevola', label: 'Thick-leaved fanflower', toml: waScaevolaToml, group: 'wa_native' },
  { id: 'wa_sheoak', label: 'Common sheoak', toml: waSheoakToml, group: 'wa_native' },
  { id: 'wa_slender_banksia', label: 'Slender / candlestick banksia', toml: waSlenderBanksiaToml, group: 'wa_native' },
  { id: 'wa_spinifex', label: 'Beach spinifex', toml: waSpinifexToml, group: 'wa_native' },
  { id: 'wa_stinkwood', label: 'Stinkwood / green stinkwood', toml: waStinkwoodToml, group: 'wa_native' },
  { id: 'wa_swamp_banksia', label: 'Swamp banksia', toml: waSwampBanksiaToml, group: 'wa_native' },
  { id: 'wa_swamp_paperbark', label: 'Swamp paperbark', toml: waSwampPaperbarkToml, group: 'wa_native' },
  { id: 'wa_swamp_sheoak', label: 'Swamp sheoak', toml: waSwampSheoakToml, group: 'wa_native' },
  { id: 'wa_sword_sedge', label: 'Coast sword-sedge', toml: waSwordSedgeToml, group: 'wa_native' },
  { id: 'wa_tuart', label: 'Tuart', toml: waTuartToml, group: 'wa_native' },
  { id: 'wa_wandoo', label: 'Wandoo', toml: waWandooToml, group: 'wa_native' },
  { id: 'wa_zamia', label: 'Zamia', toml: waZamiaToml, group: 'wa_native' },
];

/**
 * Built-in NaturePatch presets, bundled straight from the repository's
 * `presets/nature/` TOML documents — the same files `midori nature` consumes.
 */

import alpineMeadowToml from '../../../presets/nature/alpine_meadow.toml?raw';
import aridScrubToml from '../../../presets/nature/arid_scrub.toml?raw';
import floweringMeadowToml from '../../../presets/nature/flowering_meadow.toml?raw';
import pampasMeadowToml from '../../../presets/nature/pampas_meadow.toml?raw';
import perthJacarandaCarpetNatureToml from '../../../presets/nature/perth_jacaranda_carpet.toml?raw';
import temperateForestFloorToml from '../../../presets/nature/temperate_forest_floor.toml?raw';
import waBanksiaWoodlandFloorNatureToml from '../../../presets/nature/wa_banksia_woodland_floor.toml?raw';
import waJarrahForestFloorNatureToml from '../../../presets/nature/wa_jarrah_forest_floor.toml?raw';
import waCoastalDuneNatureToml from '../../../presets/nature/wa_coastal_dune.toml?raw';
import waPaperbarkWetlandEdgeNatureToml from '../../../presets/nature/wa_paperbark_wetland_edge.toml?raw';

export const NATURE_PRESETS: SpeciesPreset[] = [
  { id: 'pampas_meadow', label: 'Susuki pampas field', toml: pampasMeadowToml },
  { id: 'alpine_meadow', label: 'Alpine meadow', toml: alpineMeadowToml },
  { id: 'flowering_meadow', label: 'Flowering meadow', toml: floweringMeadowToml },
  { id: 'temperate_forest_floor', label: 'Temperate forest floor', toml: temperateForestFloorToml },
  { id: 'arid_scrub', label: 'Arid scrub', toml: aridScrubToml },
  { id: 'wa_banksia_woodland_floor', label: 'WA banksia woodland floor', toml: waBanksiaWoodlandFloorNatureToml },
  { id: 'wa_jarrah_forest_floor', label: 'WA jarrah forest floor', toml: waJarrahForestFloorNatureToml },
  { id: 'wa_coastal_dune', label: 'WA coastal dune', toml: waCoastalDuneNatureToml },
  { id: 'wa_paperbark_wetland_edge', label: 'WA paperbark wetland edge', toml: waPaperbarkWetlandEdgeNatureToml },
  { id: 'perth_jacaranda_carpet', label: 'Perth jacaranda carpet', toml: perthJacarandaCarpetNatureToml },
];
