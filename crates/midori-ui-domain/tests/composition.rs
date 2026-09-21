use midori_core::{
    ExportConfig, ExportMetadata, LodGenerationConfig, MaterialType, Mesh, Species,
    export_lod_meshes_to_bytes, generate_lod_meshes_with_config, generate_tree,
};
use midori_ui_domain::{EngineIdentity, TreeError, TreeResult, compose_tree};
use sha2::{Digest, Sha256};

const PRESETS: [&[u8]; 5] = [
    include_bytes!("../../../presets/species/oak.toml"),
    include_bytes!("../../../presets/species/pine.toml"),
    include_bytes!("../../../presets/species/palm.toml"),
    include_bytes!("../../../presets/species/willow.toml"),
    include_bytes!("../../../presets/species/joshua_prototype.toml"),
];

fn identity() -> EngineIdentity {
    // Explicit test data, not a real revision, build identity or release attestation.
    EngineIdentity {
        source_revision: "test-fixture:source-not-a-git-revision".into(),
        build_id: "test-fixture:not-release-evidence".into(),
    }
}

fn assert_mesh(left: &Mesh, right: &Mesh) {
    assert_eq!(left.indices, right.indices);
    assert_eq!(left.vertices.len(), right.vertices.len());
    for (a, b) in left.vertices.iter().zip(&right.vertices) {
        assert_eq!(a.position, b.position);
        assert_eq!(a.normal, b.normal);
        assert_eq!(a.uv, b.uv);
        assert_eq!(a.uv2, b.uv2);
        assert_eq!(a.color, b.color);
        assert!(a.position.is_finite() && a.normal.is_finite());
        assert!(a.uv.is_finite() && a.uv2.is_finite() && a.color.is_finite());
    }
    assert!(
        left.indices
            .iter()
            .all(|&i| (i as usize) < left.vertices.len())
    );
    assert_eq!(left.indices.len() % 3, 0);
    assert_eq!(left.submeshes.len(), right.submeshes.len());
    for (a, b) in left.submeshes.iter().zip(&right.submeshes) {
        assert_eq!(
            (a.index_start, a.index_count, a.material),
            (b.index_start, b.index_count, b.material)
        );
        assert!((a.index_start as usize) + (a.index_count as usize) <= left.indices.len());
        assert_eq!(a.index_count % 3, 0);
    }
}

fn assert_reopened(result: &TreeResult) {
    // Independent public gltf reader, including its normal validation path.
    let glb = gltf::Gltf::from_slice(result.glb()).expect("independent GLB parse");
    let binary = glb.blob.as_deref().expect("embedded binary");
    assert_eq!(glb.document.buffers().count(), 1);
    assert!(
        glb.document
            .buffers()
            .all(|b| matches!(b.source(), gltf::buffer::Source::Bin))
    );
    let nonempty: Vec<_> = result
        .lods()
        .meshes
        .iter()
        .filter(|lod| !lod.mesh.vertices.is_empty())
        .collect();
    assert_eq!(glb.document.meshes().count(), nonempty.len());
    assert_eq!(glb.document.nodes().count(), nonempty.len());
    assert_eq!(
        glb.document.default_scene().unwrap().nodes().count(),
        nonempty.len()
    );
    for (position, (node, mesh)) in glb.document.nodes().zip(glb.document.meshes()).enumerate() {
        assert_eq!(node.mesh().unwrap().index(), mesh.index());
        let name = node.name().expect("exported LOD node is named");
        // Bind the node to its own LOD ordinal, not merely to the "_LOD"
        // marker: emitting one LOD's name for every node must fail here.
        let expected = format!("_LOD{}", nonempty[position].index);
        assert!(
            name.ends_with(&expected),
            "node {position} named {name:?} does not carry its own LOD ordinal {expected:?}"
        );
    }
    for (exported, lod) in glb.document.meshes().zip(nonempty) {
        let mesh = &lod.mesh;
        let ranges: Vec<_> = mesh
            .submeshes
            .iter()
            .filter(|s| s.index_count > 0)
            .collect();
        assert!(!ranges.is_empty(), "preset should exercise submesh export");
        assert_eq!(exported.primitives().count(), ranges.len());
        for (primitive, range) in exported.primitives().zip(ranges) {
            assert_eq!(primitive.mode(), gltf::mesh::Mode::Triangles);
            let role = match range.material {
                MaterialType::Bark => "bark",
                MaterialType::Leaves => "leaves",
                MaterialType::Impostor => "impostor",
            };
            let name = primitive.material().name().unwrap_or_default();
            if range.material == MaterialType::Impostor {
                assert!(
                    name == "impostor" || name.starts_with("impostor_"),
                    "impostor material named {name:?}"
                );
            } else {
                assert_eq!(primitive.material().name(), Some(role));
            }
            if range.material == MaterialType::Leaves {
                assert!(primitive.material().double_sided());
                assert_eq!(
                    primitive.material().alpha_mode(),
                    gltf::material::AlphaMode::Mask
                );
            }
            let reader = primitive.reader(|buffer| (buffer.index() == 0).then_some(binary));
            assert_eq!(
                reader.read_positions().unwrap().collect::<Vec<_>>(),
                mesh.vertices
                    .iter()
                    .map(|v| v.position.to_array())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                reader.read_normals().unwrap().collect::<Vec<_>>(),
                mesh.vertices
                    .iter()
                    .map(|v| v.normal.to_array())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                reader
                    .read_tex_coords(0)
                    .unwrap()
                    .into_f32()
                    .collect::<Vec<_>>(),
                mesh.vertices
                    .iter()
                    .map(|v| v.uv.to_array())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                reader
                    .read_tex_coords(1)
                    .unwrap()
                    .into_f32()
                    .collect::<Vec<_>>(),
                mesh.vertices
                    .iter()
                    .map(|v| v.uv2.to_array())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                reader
                    .read_colors(0)
                    .unwrap()
                    .into_rgba_f32()
                    .collect::<Vec<_>>(),
                mesh.vertices
                    .iter()
                    .map(|v| v.color.to_array())
                    .collect::<Vec<_>>()
            );
            let tangents: Vec<_> = reader.read_tangents().unwrap().collect();
            assert_eq!(tangents.len(), mesh.vertices.len());
            assert!(tangents.iter().flatten().all(|v| v.is_finite()));
            let start = range.index_start as usize;
            let end = start + range.index_count as usize;
            assert_eq!(
                reader
                    .read_indices()
                    .unwrap()
                    .into_u32()
                    .collect::<Vec<_>>(),
                mesh.indices[start..end]
            );
        }
    }
    let container = gltf::binary::Glb::from_slice(result.glb()).unwrap();
    let json: serde_json::Value = serde_json::from_slice(container.json.as_ref()).unwrap();
    let metadata = &json["extras"]["midori"];
    assert_eq!(
        metadata["species_name"].as_str(),
        Some(result.summary().species_name.as_str())
    );
    assert_eq!(
        metadata["scientific_name"].as_str(),
        Some(result.summary().scientific_name.as_str())
    );
    assert_eq!(metadata["seed"].as_u64(), Some(result.seed()));
    assert_eq!(json["extras"]["pivot_painter"], true);
    let thresholds = metadata["lod_screen_heights"].as_array().unwrap();
    assert_eq!(thresholds.len(), result.lods().meshes.len());
    for (value, lod) in thresholds.iter().zip(&result.lods().meshes) {
        assert_eq!(value.as_f64().unwrap() as f32, lod.screen_height);
    }
    assert_eq!(
        result.glb_sha256(),
        &<[u8; 32]>::from(Sha256::digest(result.glb()))
    );
    assert_eq!(result.glb_byte_count(), result.glb().len() as u64);
}

#[test]
fn composition_matches_public_core_and_independent_glb_reader() {
    for source in PRESETS {
        for seed in [42_u64, 43] {
            let result = compose_tree(source, &seed.to_string(), identity()).unwrap();
            let species = Species::from_toml(std::str::from_utf8(source).unwrap()).unwrap();
            let tree = generate_tree(&species, seed).unwrap();
            let config = LodGenerationConfig::balanced();
            let expected = generate_lod_meshes_with_config(&tree, &species, &config);
            assert_eq!(result.source(), source);
            assert_eq!(
                result.source_sha256(),
                &<[u8; 32]>::from(Sha256::digest(source))
            );
            assert_eq!(result.seed_decimal(), seed.to_string());
            assert_eq!(result.engine(), &identity());
            assert_eq!(result.species().species.name, species.species.name);
            assert_eq!(result.summary().stem_count, tree.stems.len());
            assert_eq!(
                result.summary().branch_count,
                tree.stems.iter().filter(|s| s.level > 0).count()
            );
            assert_eq!(result.summary().leaf_count, tree.leaves.len());
            assert_eq!(result.summary().bounds_min, tree.bounds.min.to_array());
            assert_eq!(result.summary().bounds_max, tree.bounds.max.to_array());
            assert_eq!(
                format!("{:?}", result.lod_config()),
                format!("{:?}", config)
            );
            assert_eq!(result.lods().meshes.len(), expected.meshes.len());
            for (actual, reference) in result.lods().meshes.iter().zip(&expected.meshes) {
                assert_eq!(
                    (actual.index, &actual.name, actual.screen_height),
                    (reference.index, &reference.name, reference.screen_height)
                );
                assert_eq!(
                    format!("{:?}", actual.stats),
                    format!("{:?}", reference.stats)
                );
                assert_mesh(&actual.mesh, &reference.mesh);
            }
            let export = ExportConfig {
                metadata: Some(ExportMetadata {
                    species_name: species.species.name.clone(),
                    scientific_name: species.latin_name().into(),
                    seed: Some(seed),
                    lod_screen_heights: expected.meshes.iter().map(|l| l.screen_height).collect(),
                }),
                ..ExportConfig::default()
            };
            assert_eq!(
                result.glb(),
                export_lod_meshes_to_bytes(&expected, &export).unwrap()
            );
            assert_reopened(&result);
        }
    }
}

#[test]
fn exact_source_provenance_is_not_a_normalized_species_digest() {
    let source = PRESETS[0];
    let mut commented = b"# retained source comment\r\n".to_vec();
    commented.extend_from_slice(source);
    let first = compose_tree(source, "42", identity()).unwrap();
    let second = compose_tree(&commented, "42", identity()).unwrap();
    assert_eq!(second.source(), commented);
    assert_ne!(first.source_sha256(), second.source_sha256());
    assert_eq!(first.glb(), second.glb());
    let other = compose_tree(source, "43", identity()).unwrap();
    assert_ne!(first.glb_sha256(), other.glb_sha256());
    assert!(
        first.lods().meshes[0]
            .mesh
            .vertices
            .iter()
            .zip(&other.lods().meshes[0].mesh.vertices)
            .any(|(a, b)| a.position != b.position)
    );
}

#[test]
fn seed_boundaries_are_exact_and_errors_return_no_partial_result() {
    for seed in ["0", "9007199254740993", "18446744073709551615"] {
        let result = compose_tree(PRESETS[0], seed, identity()).unwrap();
        assert_eq!(result.seed_decimal(), seed);
        assert_reopened(&result);
    }
    for seed in [
        "",
        "00",
        "01",
        "+1",
        "-1",
        " 1",
        "1 ",
        "1.0",
        "1e3",
        "18446744073709551616",
    ] {
        assert!(matches!(
            compose_tree(PRESETS[0], seed, identity()),
            Err(TreeError::Seed)
        ));
    }
    assert!(matches!(
        compose_tree(&[0xff], "1", identity()),
        Err(TreeError::Utf8(_))
    ));
    assert!(matches!(
        compose_tree(b"not valid toml", "1", identity()),
        Err(TreeError::Species(_))
    ));
}
