/* midori C API — engine plugin surface.
 *
 * Hand-maintained to mirror crates/midori-ffi/src/lib.rs; keep in sync when
 * the FFI surface changes. Opaque handles are owned by the library: free with
 * the matching midori_*_free call, never free() directly.
 */
#ifndef MIDORI_H
#define MIDORI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque parsed-species handle. */
typedef struct MidoriSpecies MidoriSpecies;
typedef MidoriSpecies *MidoriSpeciesHandle;

/* Opaque generated-tree handle. */
typedef struct MidoriTree MidoriTree;
typedef MidoriTree *MidoriTreeHandle;

/* Status codes returned by fallible FFI calls. */
typedef enum MidoriStatus {
    MidoriStatus_Ok = 0,
    MidoriStatus_InvalidArgument = 1,
    MidoriStatus_ParseError = 2,
    MidoriStatus_ExportError = 3,
} MidoriStatus;

/* Parse a species TOML document. `toml_ptr` must point to `toml_len` readable
 * UTF-8 bytes. Returns NULL on parse failure; free with midori_species_free. */
MidoriSpeciesHandle midori_species_parse(const uint8_t *toml_ptr, size_t toml_len);

/* Free a species handle returned by midori_species_parse. */
void midori_species_free(MidoriSpeciesHandle handle);

/* Generate a tree for `species` with `seed`. Returns NULL on failure; free
 * with midori_tree_free. */
MidoriTreeHandle midori_tree_generate(MidoriSpeciesHandle species, uint64_t seed);

/* Stem/leaf counts for a generated tree (0 for a NULL handle). */
uint32_t midori_tree_stem_count(MidoriTreeHandle tree);
uint32_t midori_tree_leaf_count(MidoriTreeHandle tree);

/* Export `tree` to a GLB file at the UTF-8 path `(path_ptr, path_len)` with all
 * LOD levels from the species' [lod] configuration. Returns MidoriStatus_Ok on
 * success. */
int midori_tree_export_glb(MidoriSpeciesHandle species, MidoriTreeHandle tree,
                           const uint8_t *path_ptr, size_t path_len);

/* Like midori_tree_export_glb, with the species' material maps embedded.
 * `(base_dir_ptr, base_dir_len)` is the directory [textures] file-slot paths
 * resolve against; pass NULL/0 to generate all maps procedurally. */
int midori_tree_export_glb_textured(MidoriSpeciesHandle species, MidoriTreeHandle tree,
                                    const uint8_t *path_ptr, size_t path_len,
                                    const uint8_t *base_dir_ptr, size_t base_dir_len);

/* Free a tree handle returned by midori_tree_generate. */
void midori_tree_free(MidoriTreeHandle handle);

/* Library version string ("0.1.0"). Never NULL; do not free. */
const char *midori_version(void);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MIDORI_H */
