/* C smoke test for the midori-ffi surface: parse a species, generate a tree,
 * export a GLB, verify the magic bytes, free both handles. Exercises the real
 * link path (header + libmidori_ffi) that engine plugins would use.
 *
 * Built and run by the `ffi-smoke` CI leg; not part of `cargo test`.
 */
#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "midori.h"

static const char OAK_TOML[] =
    "[species]\n"
    "name = \"FFI Smoke Oak\"\n"
    "\n"
    "[trunk]\n"
    "height = 5.0\n"
    "radius = 0.4\n"
    "\n"
    "[branches.level1]\n"
    "count = 4\n"
    "length = 2.5\n";

int main(void)
{
    const char *version = midori_version();
    assert(version != NULL);
    printf("midori version: %s\n", version);

    MidoriSpeciesHandle species =
        midori_species_parse((const uint8_t *)OAK_TOML, strlen(OAK_TOML));
    assert(species != NULL);

    MidoriTreeHandle tree = midori_tree_generate(species, 42);
    assert(tree != NULL);
    assert(midori_tree_stem_count(tree) > 0);

    char path[512];
    snprintf(path, sizeof(path), "%s/ffi_smoke_tree.glb", ".");
    int status = midori_tree_export_glb(species, tree,
                                        (const uint8_t *)path, strlen(path));
    assert(status == MidoriStatus_Ok);

    FILE *f = fopen(path, "rb");
    assert(f != NULL);
    unsigned char magic[4] = {0};
    assert(fread(magic, 1, 4, f) == 4);
    fclose(f);
    assert(memcmp(magic, "glTF", 4) == 0);

    /* Rejection paths: nulls and bad input must not crash. */
    assert(midori_species_parse(NULL, 0) == NULL);
    assert(midori_tree_generate(NULL, 1) == NULL);
    assert(midori_tree_stem_count(NULL) == 0);

    midori_tree_free(tree);
    midori_species_free(species);

    printf("ffi smoke: ok\n");
    return 0;
}
