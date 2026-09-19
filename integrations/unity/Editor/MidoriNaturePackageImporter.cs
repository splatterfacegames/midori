#if UNITY_EDITOR
using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;

namespace Midori.Unity
{
    public static class MidoriNaturePackageImporter
    {
        private const string DefaultImportRoot = "Assets/Midori/Imported";
        private const int DetailResolutionPerPatch = 8;
        private const int MaxDetailDensity = 16;
        private const int ScatterHeaderBytes = 16;
        private const uint ScatterVersion = 1;
        private const uint ScatterRecordStride = 32;
        private const float ScatterValidationEpsilon = 0.001f;
        private const float Tau = 6.2831855f;
        private const ulong FnvOffsetBasis = 14695981039346656037UL;
        private const ulong FnvPrime = 1099511628211UL;

        [MenuItem("Tools/Midori/Import Nature Package...")]
        public static void ImportNaturePackageMenu()
        {
            var packageDirectory = EditorUtility.OpenFolderPanel(
                "Select Midori Nature Package",
                "",
                ""
            );
            if (string.IsNullOrEmpty(packageDirectory))
            {
                return;
            }

            ImportPackage(packageDirectory, DefaultImportRoot);
        }

        public static void ImportPackage(string packageDirectory, string importRoot)
        {
            ImportPackageInternal(packageDirectory, importRoot, null, null);
        }

        public static void BatchImportNaturePackage()
        {
            try
            {
                var packageDirectory = RequireCommandLineOption("-midoriPackage");
                var importRoot = GetCommandLineOption("-midoriImportRoot") ?? DefaultImportRoot;
                var reportPath = GetCommandLineOption("-midoriReport");
                var importScreenshotPath = GetCommandLineOption("-midoriImportScreenshot");
                var densityScreenshotPath = GetCommandLineOption("-midoriDensityScreenshot");
                var report = ImportPackageInternal(
                    packageDirectory,
                    importRoot,
                    importScreenshotPath,
                    densityScreenshotPath
                );

                if (!string.IsNullOrEmpty(reportPath))
                {
                    WriteReport(reportPath, report);
                    Debug.Log($"Midori Unity import report written to {reportPath}");
                }

                EditorApplication.Exit(0);
            }
            catch (Exception ex)
            {
                Debug.LogException(ex);
                EditorApplication.Exit(1);
            }
        }

        private static MidoriUnityImportReport ImportPackageInternal(
            string packageDirectory,
            string importRoot,
            string importScreenshotPath,
            string densityScreenshotPath
        )
        {
            if (!Directory.Exists(packageDirectory))
            {
                throw new DirectoryNotFoundException(packageDirectory);
            }

            var manifest = ReadManifest(Path.Combine(packageDirectory, "midori_nature.json"));
            ValidateManifest(manifest);
            var sourceFiles = CollectUnitySourceFiles(manifest);
            ValidateSourceFiles(packageDirectory, sourceFiles);
            var manifestFileChecksum = FileChecksum(
                Path.Combine(packageDirectory, "midori_nature.json")
            );
            var sourceFileChecksumXor = SourceFileChecksumXor(packageDirectory, sourceFiles);

            EnsureAssetFolder(importRoot);
            var safeName = SanitizeAssetName(manifest.asset_name);
            var packageAssetPath = AssetDatabase.GenerateUniqueAssetPath(
                $"{importRoot}/{safeName}"
            );

            FileUtil.CopyFileOrDirectory(packageDirectory, packageAssetPath);
            AssetDatabase.Refresh();

            ConfigureTextureImporters(packageAssetPath, manifest);

            var terrainData = CreateTerrainData(
                packageAssetPath,
                manifest,
                out var prototypeImportSummary
            );
            var terrainDataPath = $"{packageAssetPath}/{safeName}_Terrain.asset";
            AssetDatabase.CreateAsset(terrainData, terrainDataPath);

            var terrainObject = Terrain.CreateTerrainGameObject(terrainData);
            terrainObject.name = $"{safeName}_Terrain";
            terrainObject.transform.position = new Vector3(
                manifest.terrain.bounds_min[0],
                manifest.terrain.height_min,
                manifest.terrain.bounds_min[2]
            );

            var scatter = CreateScatterAsset(packageAssetPath, manifest);
            AssetDatabase.CreateAsset(scatter, $"{packageAssetPath}/{safeName}_Scatter.asset");
            AssetDatabase.SaveAssets();

            if (!string.IsNullOrEmpty(importScreenshotPath))
            {
                WriteTerrainScreenshot(terrainObject, terrainData, importScreenshotPath);
            }
            var importScreenshot = SummarizeScreenshotArtifact(importScreenshotPath);
            if (!string.IsNullOrEmpty(densityScreenshotPath))
            {
                WriteDensityScreenshot(packageAssetPath, manifest, densityScreenshotPath);
            }
            var densityScreenshot = SummarizeScreenshotArtifact(densityScreenshotPath);

            var report = new MidoriUnityImportReport
            {
                assetName = manifest.asset_name,
                schemaVersion = manifest.schema_version,
                profile = "mobile",
                manifestFileChecksum = ToHex(manifestFileChecksum),
                sourceFileCount = sourceFiles.Length,
                sourceFileChecksumXor = ToHex(sourceFileChecksumXor),
                sourceFiles = sourceFiles,
                importedAssetPath = packageAssetPath,
                terrainDataAsset = terrainDataPath,
                importScreenshotPath = importScreenshotPath ?? "",
                densityScreenshotPath = densityScreenshotPath ?? "",
                importScreenshot = importScreenshot,
                densityScreenshot = densityScreenshot,
                heightmapFile = manifest.terrain.heightmap_file,
                masksFile = manifest.terrain.masks_file,
                densityMapFile = manifest.terrain.grass_density_file,
                normalMapFile = manifest.normal_conventions.unity_yplus_file,
                unityHintHeightmap = manifest.unity != null ? manifest.unity.terrain_heightmap : "",
                unityHintDensityMap = manifest.unity != null ? manifest.unity.detail_density_map : "",
                unityHintNormalMap = manifest.unity != null ? manifest.unity.normal_map : "",
                unityHintDetailMode = manifest.unity != null ? manifest.unity.detail_mode : "",
                tileSizeMeters = manifest.tile_size,
                terrainSize = terrainData.size,
                terrainPosition = terrainObject.transform.position,
                terrainHeightMin = manifest.terrain.height_min,
                terrainHeightMax = manifest.terrain.height_max,
                heightmapResolution = terrainData.heightmapResolution,
                detailResolution = terrainData.detailResolution,
                detailResolutionPerPatch = DetailResolutionPerPatch,
                prototypesDeclared = manifest.prototypes.Length,
                lod0PrototypesDeclared = CountLod0Prototypes(manifest),
                lodFilesDeclared = CountLodFiles(manifest),
                detailPrototypesCreated = terrainData.detailPrototypes.Length,
                detailPrototypesLoadedFromAssets = prototypeImportSummary.loadedFromAssets,
                detailPrototypesGeneratedFromGlb = prototypeImportSummary.generatedFromGlb,
                detailPrototypeFailures = prototypeImportSummary.failures,
                detailPrototypeGeneratedFiles = prototypeImportSummary.generatedFiles.ToArray(),
                detailPrototypeFallbackErrors = prototypeImportSummary.errors.ToArray(),
                nonZeroDetailCells = CountNonZeroDetailCells(terrainData),
                scatterBinaryChunks = manifest.scatter.binary_files.Length,
                scatterBinaryInstances = scatter.instanceCount,
                scatterBinaryRecordsValidated = scatter.recordsValidated,
                scatterBinaryRecordsRead = scatter.instanceCount,
                scatterBinaryFileChecksumXor = ToHex(scatter.fileChecksumXor),
                scatterBinaryRecordChecksumXor = ToHex(scatter.recordChecksumXor),
                scatterChunkReports = scatter.chunkReports.ToArray(),
                mobileDensityScale = manifest.mobile.density_scale,
                mobileLod0MaxDistance = manifest.mobile.lod0_max_distance,
                mobileLod1MaxDistance = manifest.mobile.lod1_max_distance,
                mobileLod2MaxDistance = manifest.mobile.lod2_max_distance,
                mobileCullStartMeters = manifest.mobile.cull_start,
                mobileCullEndMeters = manifest.mobile.cull_end,
                mobileShadows = manifest.mobile.shadows,
                mobileMaterialSlots = manifest.mobile.material_slots,
                mobileMaxInstancesPerTile = manifest.mobile.max_instances_per_tile,
                mobileMaxInstancesPerChunk = manifest.mobile.max_instances_per_chunk,
                mobileGrassCollision = manifest.mobile.grass_collision,
                mobileMossCollision = manifest.mobile.moss_collision,
                materialSlotsDeclared = manifest.material_slots.Length,
                hasTerrainSurfaceMaterialSlot = HasMaterialSlot(manifest, "terrain_surface"),
                hasGroundcoverFoliageMaterialSlot = HasMaterialSlot(manifest, "groundcover_foliage"),
                materialParameterSetCount = manifest.material_parameters.Length,
                materialParameterSlots = MaterialParameterSlots(manifest),
                materialParameterRuntimePolicies = MaterialParameterRuntimePolicies(manifest),
                materialParameterSemantics = MaterialParameterSemantics(manifest),
                groundcoverMaterialParameterNames = MaterialParameterNamesForSlot(
                    manifest,
                    "groundcover_foliage"
                ),
                terrainMaterialParameterNames = MaterialParameterNamesForSlot(
                    manifest,
                    "terrain_surface"
                ),
                materialRecipeCount = manifest.material_recipes.Length,
                materialRecipeFiles = MaterialRecipeFiles(manifest),
                materialRecipeRuntimePolicies = MaterialRecipeRuntimePolicies(manifest),
                materialRecipeEngineTargets = MaterialRecipeEngineTargets(manifest),
                terrainMaterialRecipeFile = MaterialRecipeFileForSlot(manifest, "terrain_surface"),
                groundcoverMaterialRecipeFile = MaterialRecipeFileForSlot(
                    manifest,
                    "groundcover_foliage"
                ),
                engineImportRecipeCount = manifest.engine_import_recipes.Length,
                engineImportRecipeFiles = EngineImportRecipeFiles(manifest),
                engineImportRecipeProfiles = EngineImportRecipeProfiles(manifest),
                engineImportRecipeRuntimePolicies = EngineImportRecipeRuntimePolicies(manifest),
                engineImportRecipeExpectedSystems = EngineImportRecipeExpectedSystems(manifest),
                unityEngineImportRecipeFile = EngineImportRecipeFileForEngine(manifest, "unity"),
                unrealEngineImportRecipeFile = EngineImportRecipeFileForEngine(manifest, "unreal"),
                prototypeSurfaceTargets = PrototypeSurfaceTargets(manifest),
                rockPrototypeSurfaceTargets = PrototypeSurfaceTargetsForKind(manifest, "rock"),
                logPrototypeSurfaceTargets = PrototypeSurfaceTargetsForKind(manifest, "log"),
                shrubPrototypeSurfaceTargets = PrototypeSurfaceTargetsForKind(manifest, "shrub"),
                groundcoverMaterialAlphaMode = MaterialSlotValue(
                    manifest,
                    "groundcover_foliage",
                    slot => slot.alpha_mode
                ),
                groundcoverMaterialDoubleSided = MaterialSlotBool(
                    manifest,
                    "groundcover_foliage",
                    slot => slot.double_sided
                ),
                groundcoverMaterialShadows = MaterialSlotBool(
                    manifest,
                    "groundcover_foliage",
                    slot => slot.shadows
                ),
                surfaceOverlayCount = manifest.surface_overlays.Length,
                surfaceOverlayNames = SurfaceOverlayNames(manifest),
                surfaceOverlaySourceFiles = SurfaceOverlaySourceFiles(manifest),
                surfaceOverlayChannels = SurfaceOverlayChannels(manifest),
                surfaceOverlayTargets = SurfaceOverlayTargets(manifest),
                surfaceOverlayRuntimePolicies = SurfaceOverlayRuntimePolicies(manifest),
                windPhase = manifest.wind_packing != null ? manifest.wind_packing.phase : "",
                windStiffness = manifest.wind_packing != null ? manifest.wind_packing.stiffness : "",
                windHeight = manifest.wind_packing != null ? manifest.wind_packing.height : "",
                windColorVariation = manifest.wind_packing != null ? manifest.wind_packing.color_variation : "",
                windNormalizedProgress = manifest.wind_packing != null
                    ? manifest.wind_packing.normalized_progress
                    : "",
                notes = terrainData.detailPrototypes.Length == 0
                    ? new[]
                    {
                        "No Unity detail prototypes were created. Check package prototype GLBs and Unity importer logs."
                    }
                    : prototypeImportSummary.generatedFromGlb > 0
                        ? new[]
                        {
                            "Unity TerrainData and GPU-instanced detail prototypes were created; some prototype meshes were generated by Midori's native GLB fallback importer."
                        }
                    : new[]
                    {
                        "Unity TerrainData and GPU-instanced detail prototypes were created from Midori package metadata."
                    }
            };

            Selection.activeObject = terrainObject;
            Debug.Log(
                $"Imported Midori nature package '{manifest.asset_name}' with "
                    + $"{manifest.prototypes.Length} prototypes, "
                    + $"{scatter.instanceCount} scatter instances, "
                    + $"mobile density scale {manifest.mobile.density_scale:0.###}."
            );

            return report;
        }

        private static string[] CollectUnitySourceFiles(MidoriManifest manifest)
        {
            var files = new SortedSet<string>(StringComparer.Ordinal)
            {
                "midori_nature.json",
                "preview_tile.glb",
                manifest.terrain.heightmap_file,
                manifest.terrain.masks_file,
                manifest.terrain.grass_density_file,
                manifest.normal_conventions.unity_yplus_file
            };

            foreach (var prototype in manifest.prototypes)
            {
                foreach (var lod in prototype.lods)
                {
                    files.Add(lod.file);
                }
            }

            if (!string.IsNullOrEmpty(manifest.scatter.file))
            {
                files.Add(manifest.scatter.file);
            }
            foreach (var file in manifest.scatter.binary_files)
            {
                files.Add(file.file);
            }
            foreach (var recipe in manifest.material_recipes)
            {
                files.Add(recipe.file);
            }
            foreach (var recipe in manifest.engine_import_recipes)
            {
                files.Add(recipe.file);
            }

            var result = new string[files.Count];
            files.CopyTo(result);
            return result;
        }

        private static void ValidateSourceFiles(string packageDirectory, string[] sourceFiles)
        {
            foreach (var relative in sourceFiles)
            {
                if (!File.Exists(Path.Combine(packageDirectory, relative)))
                {
                    throw new FileNotFoundException("Midori source file not found.", relative);
                }
            }
        }

        private static ulong SourceFileChecksumXor(string packageDirectory, string[] sourceFiles)
        {
            var checksum = 0UL;
            foreach (var relative in sourceFiles)
            {
                checksum ^= FileChecksum(Path.Combine(packageDirectory, relative));
            }
            return checksum;
        }

        private static ulong FileChecksum(string path)
        {
            var bytes = File.ReadAllBytes(path);
            return Fnv1a(bytes, 0, bytes.Length, FnvOffsetBasis);
        }

        private static int CountLod0Prototypes(MidoriManifest manifest)
        {
            var count = 0;
            foreach (var prototype in manifest.prototypes)
            {
                if (FindLod(prototype, 0) != null)
                {
                    count++;
                }
            }
            return count;
        }

        private static int CountLodFiles(MidoriManifest manifest)
        {
            var count = 0;
            foreach (var prototype in manifest.prototypes)
            {
                count += prototype.lods.Length;
            }
            return count;
        }

        private static int CountNonZeroDetailCells(TerrainData terrainData)
        {
            var count = 0;
            for (var layer = 0; layer < terrainData.detailPrototypes.Length; layer++)
            {
                var cells = terrainData.GetDetailLayer(
                    0,
                    0,
                    terrainData.detailResolution,
                    terrainData.detailResolution,
                    layer
                );
                foreach (var value in cells)
                {
                    if (value > 0)
                    {
                        count++;
                    }
                }
            }
            return count;
        }

        private static void WriteTerrainScreenshot(
            GameObject terrainObject,
            TerrainData terrainData,
            string outputPath
        )
        {
            EnsureOutputDirectory(outputPath);

            var cameraObject = new GameObject("Midori_Validation_Camera");
            var camera = cameraObject.AddComponent<Camera>();
            var renderTexture = new RenderTexture(1024, 1024, 24, RenderTextureFormat.ARGB32);
            var previousActive = RenderTexture.active;

            try
            {
                var terrainPosition = terrainObject.transform.position;
                var terrainSize = terrainData.size;
                cameraObject.transform.position = new Vector3(
                    terrainPosition.x + terrainSize.x * 0.5f,
                    terrainPosition.y + terrainSize.y + Mathf.Max(terrainSize.x, terrainSize.z),
                    terrainPosition.z + terrainSize.z * 0.5f
                );
                cameraObject.transform.rotation = Quaternion.Euler(90.0f, 0.0f, 0.0f);
                camera.orthographic = true;
                camera.orthographicSize = Mathf.Max(terrainSize.x, terrainSize.z) * 0.58f;
                camera.nearClipPlane = 0.01f;
                camera.farClipPlane = Mathf.Max(terrainSize.x, terrainSize.z) * 3.0f;
                camera.clearFlags = CameraClearFlags.SolidColor;
                camera.backgroundColor = new Color(0.08f, 0.1f, 0.08f, 1.0f);
                camera.targetTexture = renderTexture;

                camera.Render();
                RenderTexture.active = renderTexture;
                var image = new Texture2D(1024, 1024, TextureFormat.RGBA32, false);
                image.ReadPixels(new Rect(0, 0, 1024, 1024), 0, 0);
                image.Apply();
                File.WriteAllBytes(outputPath, image.EncodeToPNG());
                UnityEngine.Object.DestroyImmediate(image);
            }
            finally
            {
                RenderTexture.active = previousActive;
                camera.targetTexture = null;
                renderTexture.Release();
                UnityEngine.Object.DestroyImmediate(renderTexture);
                UnityEngine.Object.DestroyImmediate(cameraObject);
            }
        }

        private static void WriteDensityScreenshot(
            string packageAssetPath,
            MidoriManifest manifest,
            string outputPath
        )
        {
            EnsureOutputDirectory(outputPath);

            var density = LoadTexture(JoinAssetPath(packageAssetPath, manifest.terrain.grass_density_file));
            var masks = LoadTexture(JoinAssetPath(packageAssetPath, manifest.terrain.masks_file));
            var image = new Texture2D(512, 512, TextureFormat.RGBA32, false);
            for (var y = 0; y < 512; y++)
            {
                for (var x = 0; x < 512; x++)
                {
                    var u = x / 511.0f;
                    var v = y / 511.0f;
                    var grass = density.GetPixelBilinear(u, v).grayscale;
                    var mask = masks.GetPixelBilinear(u, v);
                    image.SetPixel(
                        x,
                        y,
                        new Color(
                            Mathf.Clamp01(grass),
                            Mathf.Clamp01(mask.g),
                            Mathf.Clamp01(0.15f + mask.a * 0.85f),
                            1.0f
                        )
                    );
                }
            }
            image.Apply();
            File.WriteAllBytes(outputPath, image.EncodeToPNG());
            UnityEngine.Object.DestroyImmediate(image);
        }

        private static void EnsureOutputDirectory(string outputPath)
        {
            var directory = Path.GetDirectoryName(outputPath);
            if (!string.IsNullOrEmpty(directory))
            {
                Directory.CreateDirectory(directory);
            }
        }

        private static MidoriUnityScreenshotReport SummarizeScreenshotArtifact(string outputPath)
        {
            var report = new MidoriUnityScreenshotReport
            {
                path = outputPath ?? "",
                status = string.IsNullOrEmpty(outputPath) ? "not_requested" : "missing_after_write"
            };
            if (string.IsNullOrEmpty(outputPath) || !File.Exists(outputPath))
            {
                return report;
            }

            var info = new FileInfo(outputPath);
            report.exists = true;
            report.bytes = info.Length;
            report.checksum = ToHex(FileChecksum(outputPath));
            if (TryReadPngDimensions(outputPath, out var width, out var height))
            {
                report.status = "captured";
                report.width = width;
                report.height = height;
            }
            else
            {
                report.status = "invalid_png";
            }
            return report;
        }

        private static bool TryReadPngDimensions(string outputPath, out int width, out int height)
        {
            width = 0;
            height = 0;
            var header = new byte[24];
            using (var stream = File.OpenRead(outputPath))
            {
                if (stream.Length < header.Length)
                {
                    return false;
                }
                var read = stream.Read(header, 0, header.Length);
                if (read != header.Length)
                {
                    return false;
                }
            }

            var pngSignature = new byte[] { 137, 80, 78, 71, 13, 10, 26, 10 };
            for (var index = 0; index < pngSignature.Length; index++)
            {
                if (header[index] != pngSignature[index])
                {
                    return false;
                }
            }
            if (header[12] != (byte)'I'
                || header[13] != (byte)'H'
                || header[14] != (byte)'D'
                || header[15] != (byte)'R')
            {
                return false;
            }

            width = ReadBigEndianInt32(header, 16);
            height = ReadBigEndianInt32(header, 20);
            return width > 0 && height > 0;
        }

        private static int ReadBigEndianInt32(byte[] data, int offset)
        {
            return (data[offset] << 24)
                | (data[offset + 1] << 16)
                | (data[offset + 2] << 8)
                | data[offset + 3];
        }

        private static bool HasMaterialSlot(MidoriManifest manifest, string name)
        {
            foreach (var slot in manifest.material_slots)
            {
                if (slot.name == name)
                {
                    return true;
                }
            }
            return false;
        }

        private static string MaterialSlotValue(
            MidoriManifest manifest,
            string name,
            Func<MidoriMaterialSlot, string> selector
        )
        {
            foreach (var slot in manifest.material_slots)
            {
                if (slot.name == name)
                {
                    return selector(slot);
                }
            }
            return "";
        }

        private static bool MaterialSlotBool(
            MidoriManifest manifest,
            string name,
            Func<MidoriMaterialSlot, bool> selector
        )
        {
            foreach (var slot in manifest.material_slots)
            {
                if (slot.name == name)
                {
                    return selector(slot);
                }
            }
            return false;
        }

        private static string[] MaterialParameterSlots(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var set in manifest.material_parameters)
            {
                values.Add(set.material_slot);
            }
            return values.ToArray();
        }

        private static string[] MaterialParameterRuntimePolicies(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var set in manifest.material_parameters)
            {
                values.Add(set.runtime_policy);
            }
            return values.ToArray();
        }

        private static string[] MaterialParameterSemantics(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var set in manifest.material_parameters)
            {
                foreach (var parameter in set.parameters ?? Array.Empty<MidoriMaterialParameter>())
                {
                    values.Add($"{set.material_slot}:{parameter.semantic}");
                }
            }
            return values.ToArray();
        }

        private static string[] MaterialParameterNamesForSlot(MidoriManifest manifest, string slot)
        {
            var values = new SortedSet<string>(StringComparer.Ordinal);
            foreach (var set in manifest.material_parameters)
            {
                if (set.material_slot != slot)
                {
                    continue;
                }
                foreach (var parameter in set.parameters ?? Array.Empty<MidoriMaterialParameter>())
                {
                    values.Add(parameter.name);
                }
            }
            var result = new string[values.Count];
            values.CopyTo(result);
            return result;
        }

        private static string[] MaterialRecipeFiles(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var recipe in manifest.material_recipes)
            {
                values.Add(recipe.file);
            }
            return values.ToArray();
        }

        private static string[] MaterialRecipeRuntimePolicies(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var recipe in manifest.material_recipes)
            {
                values.Add(recipe.runtime_policy);
            }
            return values.ToArray();
        }

        private static string[] MaterialRecipeEngineTargets(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var recipe in manifest.material_recipes)
            {
                foreach (var target in recipe.engine_targets ?? Array.Empty<string>())
                {
                    values.Add($"{recipe.material_slot}:{target}");
                }
            }
            return values.ToArray();
        }

        private static string MaterialRecipeFileForSlot(MidoriManifest manifest, string slot)
        {
            foreach (var recipe in manifest.material_recipes)
            {
                if (recipe.material_slot == slot)
                {
                    return recipe.file;
                }
            }
            return "";
        }

        private static string[] EngineImportRecipeFiles(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var recipe in manifest.engine_import_recipes)
            {
                values.Add(recipe.file);
            }
            return values.ToArray();
        }

        private static string[] EngineImportRecipeProfiles(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var recipe in manifest.engine_import_recipes)
            {
                values.Add($"{recipe.engine}:{recipe.profile}");
            }
            return values.ToArray();
        }

        private static string[] EngineImportRecipeRuntimePolicies(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var recipe in manifest.engine_import_recipes)
            {
                values.Add(recipe.runtime_policy);
            }
            return values.ToArray();
        }

        private static string[] EngineImportRecipeExpectedSystems(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var recipe in manifest.engine_import_recipes)
            {
                foreach (var system in recipe.expected_systems ?? Array.Empty<string>())
                {
                    values.Add($"{recipe.engine}:{system}");
                }
            }
            return values.ToArray();
        }

        private static string EngineImportRecipeFileForEngine(MidoriManifest manifest, string engine)
        {
            foreach (var recipe in manifest.engine_import_recipes)
            {
                if (recipe.engine == engine)
                {
                    return recipe.file;
                }
            }
            return "";
        }

        private static void ValidateMaterialParameters(MidoriManifest manifest)
        {
            ValidateMaterialParameterSet(
                manifest,
                "terrain_surface",
                new[]
                {
                    "overlay_mask_texture",
                    "moss_mask_channel",
                    "wetness_mask_channel",
                    "crack_mask_channel"
                }
            );
            ValidateMaterialParameterSet(
                manifest,
                "groundcover_foliage",
                new[]
                {
                    "alpha_cutoff",
                    "wind_strength",
                    "wind_speed",
                    "wind_direction_degrees",
                    "wind_gust_scale",
                    "fade_start_meters",
                    "fade_end_meters",
                    "color_variation_scale"
                }
            );
        }

        private static void ValidateMaterialParameterSet(
            MidoriManifest manifest,
            string slot,
            string[] requiredSemantics
        )
        {
            foreach (var set in manifest.material_parameters)
            {
                if (set.material_slot != slot)
                {
                    continue;
                }
                if (set.runtime_policy != "engine_native_static")
                {
                    throw new InvalidDataException(
                        $"Midori material parameter set '{set.parameter_set}' must use engine_native_static."
                    );
                }
                var semantics = new HashSet<string>(StringComparer.Ordinal);
                foreach (var parameter in set.parameters ?? Array.Empty<MidoriMaterialParameter>())
                {
                    if (string.IsNullOrEmpty(parameter.name)
                        || string.IsNullOrEmpty(parameter.semantic)
                        || string.IsNullOrEmpty(parameter.value_type)
                        || string.IsNullOrEmpty(parameter.source)
                        || string.IsNullOrEmpty(parameter.default_value))
                    {
                        throw new InvalidDataException(
                            $"Midori material parameter set '{set.parameter_set}' has an incomplete parameter."
                        );
                    }
                    semantics.Add(parameter.semantic);
                }
                foreach (var semantic in requiredSemantics)
                {
                    if (!semantics.Contains(semantic))
                    {
                        throw new InvalidDataException(
                            $"Midori material parameter set for '{slot}' must include semantic '{semantic}'."
                        );
                    }
                }
                return;
            }

            throw new InvalidDataException(
                $"Midori material parameter metadata must include '{slot}'."
            );
        }

        private static void ValidateMaterialRecipes(MidoriManifest manifest)
        {
            ValidateMaterialRecipe(
                manifest,
                "terrain_surface",
                new[] { "unity_terrain_material", "unreal_landscape_material" }
            );
            ValidateMaterialRecipe(
                manifest,
                "groundcover_foliage",
                new[] { "unity_detail_mesh_material", "unreal_static_mesh_foliage_material" }
            );
        }

        private static void ValidateMaterialRecipe(
            MidoriManifest manifest,
            string slot,
            string[] requiredTargets
        )
        {
            foreach (var recipe in manifest.material_recipes)
            {
                if (recipe.material_slot != slot)
                {
                    continue;
                }
                if (recipe.runtime_policy != "engine_native_static")
                {
                    throw new InvalidDataException(
                        $"Midori material recipe '{recipe.file}' must use engine_native_static."
                    );
                }
                if (string.IsNullOrEmpty(recipe.file)
                    || !recipe.file.StartsWith("materials/", StringComparison.Ordinal)
                    || !recipe.file.EndsWith(".recipe.json", StringComparison.Ordinal))
                {
                    throw new InvalidDataException(
                        $"Midori material recipe for '{slot}' must live under materials/*.recipe.json."
                    );
                }
                var targets = new HashSet<string>(
                    recipe.engine_targets ?? Array.Empty<string>(),
                    StringComparer.Ordinal
                );
                foreach (var target in requiredTargets)
                {
                    if (!targets.Contains(target))
                    {
                        throw new InvalidDataException(
                            $"Midori material recipe for '{slot}' must target '{target}'."
                        );
                    }
                }
                return;
            }

            throw new InvalidDataException($"Midori material recipes must include '{slot}'.");
        }

        private static void ValidateEngineImportRecipes(MidoriManifest manifest)
        {
            ValidateEngineImportRecipe(
                manifest,
                "unity",
                "mobile",
                new[] { "Unity TerrainData", "GPU-instanced terrain detail mesh prefabs" }
            );
            ValidateEngineImportRecipe(
                manifest,
                "unreal",
                "console",
                new[] { "Unreal Landscape", "Static Mesh Foliage" }
            );
        }

        private static void ValidateEngineImportRecipe(
            MidoriManifest manifest,
            string engine,
            string profile,
            string[] requiredSystems
        )
        {
            foreach (var recipe in manifest.engine_import_recipes)
            {
                if (recipe.engine != engine)
                {
                    continue;
                }
                if (recipe.profile != profile)
                {
                    throw new InvalidDataException(
                        $"Midori engine import recipe for '{engine}' must target '{profile}'."
                    );
                }
                if (recipe.runtime_policy != "engine_native_static")
                {
                    throw new InvalidDataException(
                        $"Midori engine import recipe '{recipe.file}' must use engine_native_static."
                    );
                }
                if (string.IsNullOrEmpty(recipe.file)
                    || !recipe.file.StartsWith("engines/", StringComparison.Ordinal)
                    || !recipe.file.EndsWith(".recipe.json", StringComparison.Ordinal))
                {
                    throw new InvalidDataException(
                        $"Midori engine import recipe for '{engine}' must live under engines/*.recipe.json."
                    );
                }
                var systems = new HashSet<string>(
                    recipe.expected_systems ?? Array.Empty<string>(),
                    StringComparer.Ordinal
                );
                foreach (var system in requiredSystems)
                {
                    if (!systems.Contains(system))
                    {
                        throw new InvalidDataException(
                            $"Midori engine import recipe for '{engine}' must include system '{system}'."
                        );
                    }
                }
                return;
            }

            throw new InvalidDataException($"Midori engine import recipes must include '{engine}'.");
        }

        private static string[] SurfaceOverlayNames(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var overlay in manifest.surface_overlays)
            {
                values.Add(overlay.name);
            }
            return values.ToArray();
        }

        private static string[] SurfaceOverlaySourceFiles(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var overlay in manifest.surface_overlays)
            {
                values.Add(overlay.source_file);
            }
            return values.ToArray();
        }

        private static string[] SurfaceOverlayChannels(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var overlay in manifest.surface_overlays)
            {
                values.Add(overlay.channel);
            }
            return values.ToArray();
        }

        private static string[] SurfaceOverlayTargets(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var overlay in manifest.surface_overlays)
            {
                values.Add(string.Join(",", overlay.targets ?? Array.Empty<string>()));
            }
            return values.ToArray();
        }

        private static string[] SurfaceOverlayRuntimePolicies(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var overlay in manifest.surface_overlays)
            {
                values.Add(overlay.runtime_policy);
            }
            return values.ToArray();
        }

        private static bool HasSurfaceOverlay(MidoriManifest manifest, string name)
        {
            foreach (var overlay in manifest.surface_overlays)
            {
                if (overlay.name == name)
                {
                    return true;
                }
            }
            return false;
        }

        private static string[] PrototypeSurfaceTargets(MidoriManifest manifest)
        {
            var values = new List<string>();
            foreach (var prototype in manifest.prototypes)
            {
                values.Add(
                    $"{prototype.name}:{string.Join(",", prototype.surface_targets ?? Array.Empty<string>())}"
                );
            }
            return values.ToArray();
        }

        private static string[] PrototypeSurfaceTargetsForKind(MidoriManifest manifest, string kind)
        {
            var values = new SortedSet<string>(StringComparer.Ordinal);
            foreach (var prototype in manifest.prototypes)
            {
                if (prototype.kind != kind)
                {
                    continue;
                }
                foreach (var target in prototype.surface_targets ?? Array.Empty<string>())
                {
                    values.Add(target);
                }
            }
            var result = new string[values.Count];
            values.CopyTo(result);
            return result;
        }

        private static void ValidatePrototypeSurfaceTargets(MidoriManifest manifest)
        {
            foreach (var prototype in manifest.prototypes)
            {
                var targets = prototype.surface_targets ?? Array.Empty<string>();
                if (targets.Length == 0)
                {
                    throw new InvalidDataException(
                        $"Midori prototype '{prototype.name}' is missing surface target metadata."
                    );
                }

                foreach (var required in RequiredSurfaceTargetsForKind(prototype.kind))
                {
                    if (Array.IndexOf(targets, required) < 0)
                    {
                        throw new InvalidDataException(
                            $"Midori prototype '{prototype.name}' kind '{prototype.kind}' must include surface target '{required}'."
                        );
                    }
                }
            }
        }

        private static string[] RequiredSurfaceTargetsForKind(string kind)
        {
            switch (kind)
            {
                case "grass":
                case "flower":
                case "weed":
                case "litter":
                    return new[] { "groundcover_foliage" };
                case "moss":
                    return new[] { "groundcover_foliage", "moss_tuft" };
                case "shrub":
                    return new[] { "groundcover_foliage", "shrub_base" };
                case "rock":
                    return new[] { "static_surface", "rock" };
                case "log":
                    return new[] { "static_surface", "log" };
                default:
                    return Array.Empty<string>();
            }
        }

        private static MidoriManifest ReadManifest(string manifestPath)
        {
            if (!File.Exists(manifestPath))
            {
                throw new FileNotFoundException("Midori manifest not found", manifestPath);
            }

            var manifest = JsonUtility.FromJson<MidoriManifest>(File.ReadAllText(manifestPath));
            if (manifest == null)
            {
                throw new InvalidDataException("Could not parse Midori manifest JSON.");
            }
            return manifest;
        }

        private static void ValidateManifest(MidoriManifest manifest)
        {
            if (manifest.schema_version != 3)
            {
                throw new InvalidDataException(
                    $"Midori schema version 3 is required, got {manifest.schema_version}."
                );
            }
            if (manifest.shader_policy != "preview_only")
            {
                throw new InvalidDataException("Midori exported shader policy must be preview_only.");
            }
            if (manifest.texture_pipeline != "parked")
            {
                throw new InvalidDataException("Midori texture/PBR pipeline must remain parked.");
            }
            ValidateMaterialParameters(manifest);
            ValidateMaterialRecipes(manifest);
            ValidateEngineImportRecipes(manifest);
            if (!HasSurfaceOverlay(manifest, "moss")
                || !HasSurfaceOverlay(manifest, "wetness")
                || !HasSurfaceOverlay(manifest, "cracks"))
            {
                throw new InvalidDataException(
                    "Midori surface overlay metadata must include moss, wetness, and cracks."
                );
            }
            ValidatePrototypeSurfaceTargets(manifest);
            if (manifest.terrain == null || manifest.terrain.height_max < manifest.terrain.height_min)
            {
                throw new InvalidDataException("Midori terrain height range is missing or invalid.");
            }
            if (manifest.scatter == null
                || manifest.scatter.binary_format == null
                || manifest.scatter.binary_format.format != "midori.scatter.bin.v1"
                || manifest.scatter.binary_format.header_bytes != 16
                || manifest.scatter.binary_format.record_stride_bytes != 32
                || manifest.scatter.binary_format.endian != "little")
            {
                throw new InvalidDataException("Midori binary scatter format is not v1.");
            }
        }

        private static void ConfigureTextureImporters(string packageAssetPath, MidoriManifest manifest)
        {
            ConfigureTexture(
                JoinAssetPath(packageAssetPath, manifest.terrain.heightmap_file),
                TextureImporterType.Default,
                true
            );
            ConfigureTexture(
                JoinAssetPath(packageAssetPath, manifest.terrain.masks_file),
                TextureImporterType.Default,
                true
            );
            ConfigureTexture(
                JoinAssetPath(packageAssetPath, manifest.terrain.grass_density_file),
                TextureImporterType.Default,
                true
            );
            ConfigureTexture(
                JoinAssetPath(packageAssetPath, manifest.normal_conventions.unity_yplus_file),
                TextureImporterType.NormalMap,
                false
            );
        }

        private static void ConfigureTexture(string assetPath, TextureImporterType type, bool readable)
        {
            var importer = AssetImporter.GetAtPath(assetPath) as TextureImporter;
            if (importer == null)
            {
                Debug.LogWarning($"Midori texture not imported yet: {assetPath}");
                return;
            }

            importer.textureType = type;
            importer.sRGBTexture = false;
            importer.isReadable = readable;
            importer.textureCompression = TextureImporterCompression.Uncompressed;
            importer.SaveAndReimport();
        }

        private static TerrainData CreateTerrainData(
            string packageAssetPath,
            MidoriManifest manifest,
            out MidoriUnityPrototypeImportSummary prototypeImportSummary
        )
        {
            var heightTexture = LoadTexture(JoinAssetPath(packageAssetPath, manifest.terrain.heightmap_file));
            var heightmapResolution = ToUnityHeightmapResolution(manifest.map_resolution);
            var terrainData = new TerrainData
            {
                heightmapResolution = heightmapResolution,
                size = new Vector3(
                    manifest.tile_size,
                    Mathf.Max(0.01f, manifest.terrain.height_max - manifest.terrain.height_min),
                    manifest.tile_size
                )
            };
            terrainData.SetHeights(0, 0, ReadHeightGrid(heightTexture, heightmapResolution));

            var prototypes = BuildDetailPrototypes(packageAssetPath, manifest, out prototypeImportSummary);
            if (prototypes.Count > 0)
            {
                var detailResolution = Mathf.Max(DetailResolutionPerPatch, manifest.map_resolution);
                terrainData.SetDetailResolution(detailResolution, DetailResolutionPerPatch);
                terrainData.detailPrototypes = prototypes.ToArray();
                terrainData.RefreshPrototypes();
                ApplyDetailLayers(terrainData, packageAssetPath, manifest, detailResolution, prototypes.Count);
            }

            return terrainData;
        }

        private static List<DetailPrototype> BuildDetailPrototypes(
            string packageAssetPath,
            MidoriManifest manifest,
            out MidoriUnityPrototypeImportSummary importSummary
        )
        {
            importSummary = new MidoriUnityPrototypeImportSummary();
            var prototypes = new List<DetailPrototype>();
            foreach (var prototype in manifest.prototypes)
            {
                var lod0 = FindLod(prototype, 0);
                if (lod0 == null)
                {
                    continue;
                }

                var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(
                    JoinAssetPath(packageAssetPath, lod0.file)
                );
                if (prefab != null)
                {
                    importSummary.loadedFromAssets++;
                }
                if (prefab == null)
                {
                    prefab = CreateNativeGlbPrefab(packageAssetPath, prototype, lod0, importSummary);
                    if (prefab == null)
                    {
                        importSummary.failures++;
                        Debug.LogWarning(
                            $"Midori prototype '{prototype.name}' could not be loaded from GLB by AssetDatabase "
                                + "or Midori's native fallback importer."
                        );
                        continue;
                    }
                    importSummary.generatedFromGlb++;
                    importSummary.generatedFiles.Add(lod0.file);
                }

                prototypes.Add(new DetailPrototype
                {
                    prototype = prefab,
                    usePrototypeMesh = true,
                    useInstancing = true,
                    renderMode = DetailRenderMode.VertexLit,
                    minWidth = 0.8f,
                    maxWidth = 1.2f,
                    minHeight = 0.8f,
                    maxHeight = 1.25f,
                    healthyColor = Color.white,
                    dryColor = Color.white
                });
            }
            return prototypes;
        }

        private static GameObject CreateNativeGlbPrefab(
            string packageAssetPath,
            MidoriPrototype prototype,
            MidoriPrototypeLod lod,
            MidoriUnityPrototypeImportSummary importSummary
        )
        {
            try
            {
                var mesh = LoadNativeGlbMesh(AssetPathToFullPath(JoinAssetPath(packageAssetPath, lod.file)));
                mesh.name = $"{SanitizeAssetName(prototype.name)}_LOD{lod.index}_Mesh";
                var generatedRoot = JoinAssetPath(packageAssetPath, "UnityGenerated");
                EnsureAssetFolder(generatedRoot);

                var meshPath = AssetDatabase.GenerateUniqueAssetPath(
                    $"{generatedRoot}/{mesh.name}.asset"
                );
                AssetDatabase.CreateAsset(mesh, meshPath);

                var material = GetOrCreateGroundcoverMaterial(generatedRoot);
                var gameObject = new GameObject($"{SanitizeAssetName(prototype.name)}_LOD{lod.index}");
                gameObject.AddComponent<MeshFilter>().sharedMesh = mesh;
                gameObject.AddComponent<MeshRenderer>().sharedMaterial = material;

                var prefabPath = AssetDatabase.GenerateUniqueAssetPath(
                    $"{generatedRoot}/{gameObject.name}.prefab"
                );
                var prefab = PrefabUtility.SaveAsPrefabAsset(gameObject, prefabPath);
                UnityEngine.Object.DestroyImmediate(gameObject);
                return prefab;
            }
            catch (Exception ex)
            {
                importSummary.errors.Add($"{lod.file}: {ex.Message}");
                Debug.LogWarning($"Midori native GLB fallback failed for '{lod.file}': {ex.Message}");
                return null;
            }
        }

        private static Material GetOrCreateGroundcoverMaterial(string generatedRoot)
        {
            var materialPath = $"{generatedRoot}/Midori_Groundcover_Foliage.mat";
            var material = AssetDatabase.LoadAssetAtPath<Material>(materialPath);
            if (material != null)
            {
                return material;
            }

            var shader = Shader.Find("Universal Render Pipeline/Lit")
                ?? Shader.Find("Standard")
                ?? Shader.Find("Unlit/Color");
            material = new Material(shader)
            {
                name = "Midori_Groundcover_Foliage",
                color = new Color(0.45f, 0.62f, 0.28f, 1.0f)
            };
            AssetDatabase.CreateAsset(material, materialPath);
            return material;
        }

        private static Mesh LoadNativeGlbMesh(string path)
        {
            var bytes = File.ReadAllBytes(path);
            if (bytes.Length < 20
                || bytes[0] != (byte)'g'
                || bytes[1] != (byte)'l'
                || bytes[2] != (byte)'T'
                || bytes[3] != (byte)'F')
            {
                throw new InvalidDataException("not a GLB file");
            }

            var version = ReadUInt32LittleEndian(bytes, 4);
            if (version != 2)
            {
                throw new InvalidDataException($"unsupported GLB version {version}");
            }

            var offset = 12;
            string json = null;
            byte[] binary = null;
            while (offset + 8 <= bytes.Length)
            {
                var chunkLength = checked((int)ReadUInt32LittleEndian(bytes, offset));
                var chunkType = ReadUInt32LittleEndian(bytes, offset + 4);
                var chunkStart = offset + 8;
                var chunkEnd = chunkStart + chunkLength;
                if (chunkEnd > bytes.Length)
                {
                    throw new InvalidDataException("truncated GLB chunk");
                }

                if (chunkType == 0x4E4F534A)
                {
                    json = Encoding.UTF8.GetString(bytes, chunkStart, chunkLength).TrimEnd('\0', ' ', '\n', '\r', '\t');
                }
                else if (chunkType == 0x004E4942)
                {
                    binary = new byte[chunkLength];
                    Buffer.BlockCopy(bytes, chunkStart, binary, 0, chunkLength);
                }

                offset = chunkEnd;
            }

            if (string.IsNullOrEmpty(json) || binary == null)
            {
                throw new InvalidDataException("GLB is missing JSON or BIN chunks");
            }

            var gltf = JsonUtility.FromJson<MidoriGltfRoot>(json);
            if (gltf == null || gltf.meshes == null || gltf.meshes.Length == 0)
            {
                throw new InvalidDataException("GLB has no meshes");
            }

            return BuildUnityMesh(gltf, binary);
        }

        private static Mesh BuildUnityMesh(MidoriGltfRoot gltf, byte[] binary)
        {
            var positions = new List<Vector3>();
            var normals = new List<Vector3>();
            var uv0 = new List<Vector2>();
            var uv1 = new List<Vector2>();
            var tangents = new List<Vector4>();
            var colors = new List<Color>();
            var indices = new List<int>();

            foreach (var mesh in gltf.meshes)
            {
                if (mesh.primitives == null)
                {
                    continue;
                }
                foreach (var primitive in mesh.primitives)
                {
                    if (primitive.attributes == null || primitive.attributes.POSITION < 0)
                    {
                        continue;
                    }

                    var baseVertex = positions.Count;
                    var primitivePositions = ReadVec3Accessor(gltf, binary, primitive.attributes.POSITION);
                    positions.AddRange(primitivePositions);
                    if (primitive.attributes.NORMAL >= 0)
                    {
                        normals.AddRange(ReadVec3Accessor(gltf, binary, primitive.attributes.NORMAL));
                    }
                    if (primitive.attributes.TEXCOORD_0 >= 0)
                    {
                        uv0.AddRange(ReadVec2Accessor(gltf, binary, primitive.attributes.TEXCOORD_0));
                    }
                    if (primitive.attributes.TEXCOORD_1 >= 0)
                    {
                        uv1.AddRange(ReadVec2Accessor(gltf, binary, primitive.attributes.TEXCOORD_1));
                    }
                    if (primitive.attributes.TANGENT >= 0)
                    {
                        tangents.AddRange(ReadVec4Accessor(gltf, binary, primitive.attributes.TANGENT));
                    }
                    if (primitive.attributes.COLOR_0 >= 0)
                    {
                        colors.AddRange(ReadColorAccessor(gltf, binary, primitive.attributes.COLOR_0));
                    }

                    if (primitive.indices >= 0)
                    {
                        foreach (var index in ReadIndexAccessor(gltf, binary, primitive.indices))
                        {
                            indices.Add(baseVertex + index);
                        }
                    }
                    else
                    {
                        for (var index = 0; index < primitivePositions.Length; index++)
                        {
                            indices.Add(baseVertex + index);
                        }
                    }
                }
            }

            if (positions.Count == 0 || indices.Count == 0)
            {
                throw new InvalidDataException("GLB mesh has no positions or indices");
            }

            var unityMesh = new Mesh
            {
                indexFormat = positions.Count > 65535 ? IndexFormat.UInt32 : IndexFormat.UInt16
            };
            unityMesh.SetVertices(positions);
            if (normals.Count == positions.Count)
            {
                unityMesh.SetNormals(normals);
            }
            if (uv0.Count == positions.Count)
            {
                unityMesh.SetUVs(0, uv0);
            }
            if (uv1.Count == positions.Count)
            {
                unityMesh.SetUVs(1, uv1);
            }
            if (tangents.Count == positions.Count)
            {
                unityMesh.SetTangents(tangents);
            }
            if (colors.Count == positions.Count)
            {
                unityMesh.SetColors(colors);
            }
            unityMesh.SetTriangles(indices, 0);
            if (normals.Count != positions.Count)
            {
                unityMesh.RecalculateNormals();
            }
            unityMesh.RecalculateBounds();
            return unityMesh;
        }

        private static Vector3[] ReadVec3Accessor(MidoriGltfRoot gltf, byte[] binary, int accessorIndex)
        {
            var accessor = GetAccessor(gltf, accessorIndex, 5126, "VEC3");
            var result = new Vector3[accessor.count];
            var stride = AccessorStride(gltf, accessor, 12);
            var offset = AccessorByteOffset(gltf, accessor);
            for (var index = 0; index < accessor.count; index++)
            {
                var cursor = offset + index * stride;
                result[index] = new Vector3(
                    ReadSingleLittleEndian(binary, cursor),
                    ReadSingleLittleEndian(binary, cursor + 4),
                    ReadSingleLittleEndian(binary, cursor + 8)
                );
            }
            return result;
        }

        private static Vector2[] ReadVec2Accessor(MidoriGltfRoot gltf, byte[] binary, int accessorIndex)
        {
            var accessor = GetAccessor(gltf, accessorIndex, 5126, "VEC2");
            var result = new Vector2[accessor.count];
            var stride = AccessorStride(gltf, accessor, 8);
            var offset = AccessorByteOffset(gltf, accessor);
            for (var index = 0; index < accessor.count; index++)
            {
                var cursor = offset + index * stride;
                result[index] = new Vector2(
                    ReadSingleLittleEndian(binary, cursor),
                    ReadSingleLittleEndian(binary, cursor + 4)
                );
            }
            return result;
        }

        private static Vector4[] ReadVec4Accessor(MidoriGltfRoot gltf, byte[] binary, int accessorIndex)
        {
            var accessor = GetAccessor(gltf, accessorIndex, 5126, "VEC4");
            var result = new Vector4[accessor.count];
            var stride = AccessorStride(gltf, accessor, 16);
            var offset = AccessorByteOffset(gltf, accessor);
            for (var index = 0; index < accessor.count; index++)
            {
                var cursor = offset + index * stride;
                result[index] = new Vector4(
                    ReadSingleLittleEndian(binary, cursor),
                    ReadSingleLittleEndian(binary, cursor + 4),
                    ReadSingleLittleEndian(binary, cursor + 8),
                    ReadSingleLittleEndian(binary, cursor + 12)
                );
            }
            return result;
        }

        private static Color[] ReadColorAccessor(MidoriGltfRoot gltf, byte[] binary, int accessorIndex)
        {
            var accessor = GetAccessor(gltf, accessorIndex, 5126, null);
            var isRgb = accessor.type == "VEC3";
            var isRgba = accessor.type == "VEC4";
            if (!isRgb && !isRgba)
            {
                throw new InvalidDataException($"unsupported COLOR_0 accessor type {accessor.type}");
            }

            var componentCount = isRgb ? 3 : 4;
            var result = new Color[accessor.count];
            var stride = AccessorStride(gltf, accessor, componentCount * 4);
            var offset = AccessorByteOffset(gltf, accessor);
            for (var index = 0; index < accessor.count; index++)
            {
                var cursor = offset + index * stride;
                result[index] = new Color(
                    ReadSingleLittleEndian(binary, cursor),
                    ReadSingleLittleEndian(binary, cursor + 4),
                    ReadSingleLittleEndian(binary, cursor + 8),
                    componentCount == 4 ? ReadSingleLittleEndian(binary, cursor + 12) : 1.0f
                );
            }
            return result;
        }

        private static int[] ReadIndexAccessor(MidoriGltfRoot gltf, byte[] binary, int accessorIndex)
        {
            var accessor = GetAccessor(gltf, accessorIndex, null, "SCALAR");
            var result = new int[accessor.count];
            var componentBytes = accessor.componentType == 5125 ? 4 : accessor.componentType == 5123 ? 2 : 1;
            var stride = AccessorStride(gltf, accessor, componentBytes);
            var offset = AccessorByteOffset(gltf, accessor);
            for (var index = 0; index < accessor.count; index++)
            {
                var cursor = offset + index * stride;
                if (accessor.componentType == 5125)
                {
                    result[index] = checked((int)ReadUInt32LittleEndian(binary, cursor));
                }
                else if (accessor.componentType == 5123)
                {
                    result[index] = binary[cursor] | binary[cursor + 1] << 8;
                }
                else if (accessor.componentType == 5121)
                {
                    result[index] = binary[cursor];
                }
                else
                {
                    throw new InvalidDataException($"unsupported index component type {accessor.componentType}");
                }
            }
            return result;
        }

        private static MidoriGltfAccessor GetAccessor(
            MidoriGltfRoot gltf,
            int accessorIndex,
            int? componentType,
            string type
        )
        {
            if (accessorIndex < 0 || gltf.accessors == null || accessorIndex >= gltf.accessors.Length)
            {
                throw new InvalidDataException($"invalid accessor index {accessorIndex}");
            }
            var accessor = gltf.accessors[accessorIndex];
            if (componentType.HasValue && accessor.componentType != componentType.Value)
            {
                throw new InvalidDataException($"accessor {accessorIndex} has unsupported component type {accessor.componentType}");
            }
            if (type != null && accessor.type != type)
            {
                throw new InvalidDataException($"accessor {accessorIndex} has unsupported type {accessor.type}");
            }
            return accessor;
        }

        private static int AccessorByteOffset(MidoriGltfRoot gltf, MidoriGltfAccessor accessor)
        {
            if (gltf.bufferViews == null
                || accessor.bufferView < 0
                || accessor.bufferView >= gltf.bufferViews.Length)
            {
                throw new InvalidDataException($"invalid bufferView index {accessor.bufferView}");
            }
            var view = gltf.bufferViews[accessor.bufferView];
            return view.byteOffset + accessor.byteOffset;
        }

        private static int AccessorStride(MidoriGltfRoot gltf, MidoriGltfAccessor accessor, int fallback)
        {
            if (gltf.bufferViews == null
                || accessor.bufferView < 0
                || accessor.bufferView >= gltf.bufferViews.Length)
            {
                throw new InvalidDataException($"invalid bufferView index {accessor.bufferView}");
            }
            var stride = gltf.bufferViews[accessor.bufferView].byteStride;
            return stride > 0 ? stride : fallback;
        }

        private static void ApplyDetailLayers(
            TerrainData terrainData,
            string packageAssetPath,
            MidoriManifest manifest,
            int detailResolution,
            int prototypeCount
        )
        {
            var grassDensity = LoadTexture(JoinAssetPath(packageAssetPath, manifest.terrain.grass_density_file));
            var masks = LoadTexture(JoinAssetPath(packageAssetPath, manifest.terrain.masks_file));
            var densityScale = Mathf.Clamp01(manifest.mobile.density_scale);

            for (var prototypeIndex = 0; prototypeIndex < prototypeCount; prototypeIndex++)
            {
                var prototype = manifest.prototypes[prototypeIndex];
                var layer = new int[detailResolution, detailResolution];
                for (var z = 0; z < detailResolution; z++)
                {
                    for (var x = 0; x < detailResolution; x++)
                    {
                        var u = detailResolution <= 1 ? 0.0f : x / (float)(detailResolution - 1);
                        var v = detailResolution <= 1 ? 0.0f : z / (float)(detailResolution - 1);
                        var coverage = prototype.kind == "moss"
                            ? masks.GetPixelBilinear(u, v).g
                            : grassDensity.GetPixelBilinear(u, v).grayscale;
                        layer[z, x] = Mathf.RoundToInt(coverage * densityScale * MaxDetailDensity);
                    }
                }
                terrainData.SetDetailLayer(0, 0, prototypeIndex, layer);
            }
        }

        private static MidoriNatureScatterAsset CreateScatterAsset(
            string packageAssetPath,
            MidoriManifest manifest
        )
        {
            var scatter = ScriptableObject.CreateInstance<MidoriNatureScatterAsset>();
            scatter.packageName = manifest.asset_name;
            scatter.schemaVersion = manifest.schema_version;

            foreach (var file in manifest.scatter.binary_files)
            {
                var chunk = ReadScatterChunk(packageAssetPath, file);
                scatter.instanceCount += chunk.instances.Count;
                scatter.fileChecksumXor ^= chunk.fileChecksumValue;
                scatter.recordChecksumXor ^= chunk.recordChecksumValue;
                scatter.chunkReports.Add(chunk.report);
                scatter.chunks.Add(chunk);
            }
            scatter.recordsValidated = true;

            return scatter;
        }

        private static MidoriScatterChunk ReadScatterChunk(
            string packageAssetPath,
            MidoriScatterBinaryFile file
        )
        {
            var fullPath = AssetPathToFullPath(JoinAssetPath(packageAssetPath, file.file));
            var bytes = File.ReadAllBytes(fullPath);
            if (bytes.Length < ScatterHeaderBytes)
            {
                throw new InvalidDataException($"Midori scatter binary {file.file} is too short.");
            }

            var magic = Encoding.ASCII.GetString(bytes, 0, 4);
            if (magic != "MDSI")
            {
                throw new InvalidDataException($"Invalid Midori scatter binary magic in {file.file}.");
            }

            var version = ReadUInt32LittleEndian(bytes, 4);
            var stride = ReadUInt32LittleEndian(bytes, 8);
            var count = ReadUInt32LittleEndian(bytes, 12);
            if (version != ScatterVersion || stride != ScatterRecordStride || count != file.instance_count)
            {
                throw new InvalidDataException($"Invalid Midori scatter binary header in {file.file}.");
            }
            if (count == 0)
            {
                throw new InvalidDataException($"Midori scatter binary {file.file} must not be empty.");
            }
            if (count > int.MaxValue)
            {
                throw new InvalidDataException($"Midori scatter binary {file.file} has too many records.");
            }
            var maxRecordCount = (uint)((int.MaxValue - ScatterHeaderBytes) / (int)ScatterRecordStride);
            if (count > maxRecordCount)
            {
                throw new InvalidDataException($"Midori scatter binary {file.file} is too large.");
            }
            var recordCount = (int)count;

            var expectedLength = ScatterHeaderBytes + recordCount * (int)ScatterRecordStride;
            if (bytes.Length != expectedLength)
            {
                throw new InvalidDataException(
                    $"Midori scatter binary {file.file} length {bytes.Length} != {expectedLength}."
                );
            }

            var chunk = new MidoriScatterChunk
            {
                file = file.file,
                layerName = file.layer_name,
                kind = file.kind,
                chunkX = file.chunk_x,
                chunkZ = file.chunk_z,
                boundsMin = ToVector3(file.bounds_min),
                boundsMax = ToVector3(file.bounds_max)
            };

            var positionMin = new Vector3(float.MaxValue, float.MaxValue, float.MaxValue);
            var positionMax = new Vector3(float.MinValue, float.MinValue, float.MinValue);
            var yawMin = float.MaxValue;
            var yawMax = float.MinValue;
            var heightMin = float.MaxValue;
            var heightMax = float.MinValue;
            var widthMin = float.MaxValue;
            var widthMax = float.MinValue;
            var phaseMin = float.MaxValue;
            var phaseMax = float.MinValue;
            var colorVariationMin = float.MaxValue;
            var colorVariationMax = float.MinValue;
            var recordChecksum = FnvOffsetBasis;

            for (var index = 0; index < recordCount; index++)
            {
                var offset = ScatterHeaderBytes + index * (int)ScatterRecordStride;
                var instance = new MidoriScatterInstance
                {
                    position = new Vector3(
                        ReadSingleLittleEndian(bytes, offset),
                        ReadSingleLittleEndian(bytes, offset + 4),
                        ReadSingleLittleEndian(bytes, offset + 8)
                    ),
                    yawRadians = ReadSingleLittleEndian(bytes, offset + 12),
                    heightMultiplier = ReadSingleLittleEndian(bytes, offset + 16),
                    widthMultiplier = ReadSingleLittleEndian(bytes, offset + 20),
                    phaseRadians = ReadSingleLittleEndian(bytes, offset + 24),
                    colorVariation = ReadSingleLittleEndian(bytes, offset + 28)
                };
                ValidateScatterInstance(instance, chunk.boundsMin, chunk.boundsMax, file.file);
                chunk.instances.Add(instance);

                positionMin = Vector3.Min(positionMin, instance.position);
                positionMax = Vector3.Max(positionMax, instance.position);
                yawMin = Mathf.Min(yawMin, instance.yawRadians);
                yawMax = Mathf.Max(yawMax, instance.yawRadians);
                heightMin = Mathf.Min(heightMin, instance.heightMultiplier);
                heightMax = Mathf.Max(heightMax, instance.heightMultiplier);
                widthMin = Mathf.Min(widthMin, instance.widthMultiplier);
                widthMax = Mathf.Max(widthMax, instance.widthMultiplier);
                phaseMin = Mathf.Min(phaseMin, instance.phaseRadians);
                phaseMax = Mathf.Max(phaseMax, instance.phaseRadians);
                colorVariationMin = Mathf.Min(colorVariationMin, instance.colorVariation);
                colorVariationMax = Mathf.Max(colorVariationMax, instance.colorVariation);

                recordChecksum = Fnv1a(bytes, offset, (int)ScatterRecordStride, recordChecksum);
            }

            chunk.fileChecksumValue = Fnv1a(bytes, 0, bytes.Length, FnvOffsetBasis);
            chunk.recordChecksumValue = recordChecksum;
            chunk.report = new MidoriUnityScatterChunkReport
            {
                file = file.file,
                layerName = file.layer_name,
                kind = file.kind,
                chunkX = file.chunk_x,
                chunkZ = file.chunk_z,
                instanceCount = recordCount,
                boundsMin = chunk.boundsMin,
                boundsMax = chunk.boundsMax,
                positionMin = positionMin,
                positionMax = positionMax,
                yawMin = yawMin,
                yawMax = yawMax,
                heightMin = heightMin,
                heightMax = heightMax,
                widthMin = widthMin,
                widthMax = widthMax,
                phaseMin = phaseMin,
                phaseMax = phaseMax,
                colorVariationMin = colorVariationMin,
                colorVariationMax = colorVariationMax,
                fileChecksum = ToHex(chunk.fileChecksumValue),
                recordChecksum = ToHex(chunk.recordChecksumValue)
            };

            return chunk;
        }

        private static void ValidateScatterInstance(
            MidoriScatterInstance instance,
            Vector3 boundsMin,
            Vector3 boundsMax,
            string file
        )
        {
            if (!IsFinite(instance.position.x)
                || !IsFinite(instance.position.y)
                || !IsFinite(instance.position.z)
                || instance.position.x < boundsMin.x - ScatterValidationEpsilon
                || instance.position.x > boundsMax.x + ScatterValidationEpsilon
                || instance.position.y < boundsMin.y - ScatterValidationEpsilon
                || instance.position.y > boundsMax.y + ScatterValidationEpsilon
                || instance.position.z < boundsMin.z - ScatterValidationEpsilon
                || instance.position.z > boundsMax.z + ScatterValidationEpsilon)
            {
                throw new InvalidDataException($"{file} contains an out-of-bounds scatter instance.");
            }
            if (!IsFinite(instance.yawRadians) || instance.yawRadians < 0.0f || instance.yawRadians > Tau)
            {
                throw new InvalidDataException($"{file} contains invalid scatter yaw.");
            }
            if (!IsFinite(instance.heightMultiplier) || instance.heightMultiplier <= 0.0f)
            {
                throw new InvalidDataException($"{file} contains invalid scatter height multiplier.");
            }
            if (!IsFinite(instance.widthMultiplier) || instance.widthMultiplier <= 0.0f)
            {
                throw new InvalidDataException($"{file} contains invalid scatter width multiplier.");
            }
            if (!IsFinite(instance.phaseRadians) || instance.phaseRadians < 0.0f || instance.phaseRadians > Tau)
            {
                throw new InvalidDataException($"{file} contains invalid scatter phase.");
            }
            if (!IsFinite(instance.colorVariation)
                || instance.colorVariation < 0.0f
                || instance.colorVariation > 1.0f)
            {
                throw new InvalidDataException($"{file} contains invalid scatter color variation.");
            }
        }

        private static bool IsFinite(float value)
        {
            return !float.IsNaN(value) && !float.IsInfinity(value);
        }

        private static uint ReadUInt32LittleEndian(byte[] bytes, int offset)
        {
            return (uint)(
                bytes[offset]
                | bytes[offset + 1] << 8
                | bytes[offset + 2] << 16
                | bytes[offset + 3] << 24
            );
        }

        private static float ReadSingleLittleEndian(byte[] bytes, int offset)
        {
            var valueBytes = new byte[4];
            Buffer.BlockCopy(bytes, offset, valueBytes, 0, 4);
            if (!BitConverter.IsLittleEndian)
            {
                Array.Reverse(valueBytes);
            }
            return BitConverter.ToSingle(valueBytes, 0);
        }

        private static ulong Fnv1a(byte[] bytes, int offset, int length, ulong seed)
        {
            var hash = seed;
            for (var index = offset; index < offset + length; index++)
            {
                hash ^= bytes[index];
                hash *= FnvPrime;
            }
            return hash;
        }

        private static string ToHex(ulong value)
        {
            return $"0x{value:x16}";
        }

        private static float[,] ReadHeightGrid(Texture2D texture, int resolution)
        {
            var heights = new float[resolution, resolution];
            for (var z = 0; z < resolution; z++)
            {
                for (var x = 0; x < resolution; x++)
                {
                    var u = resolution <= 1 ? 0.0f : x / (float)(resolution - 1);
                    var v = resolution <= 1 ? 0.0f : z / (float)(resolution - 1);
                    heights[z, x] = texture.GetPixelBilinear(u, v).grayscale;
                }
            }
            return heights;
        }

        private static int ToUnityHeightmapResolution(int midoriResolution)
        {
            var samples = Mathf.Max(2, midoriResolution);
            var intervals = Mathf.NextPowerOfTwo(samples - 1);
            return Mathf.Max(33, intervals + 1);
        }

        private static MidoriPrototypeLod FindLod(MidoriPrototype prototype, int index)
        {
            foreach (var lod in prototype.lods)
            {
                if (lod.index == index)
                {
                    return lod;
                }
            }
            return null;
        }

        private static Texture2D LoadTexture(string assetPath)
        {
            var texture = AssetDatabase.LoadAssetAtPath<Texture2D>(assetPath);
            if (texture == null)
            {
                throw new FileNotFoundException($"Texture asset not found: {assetPath}");
            }
            return texture;
        }

        private static string GetCommandLineOption(string name)
        {
            var args = Environment.GetCommandLineArgs();
            for (var index = 0; index < args.Length - 1; index++)
            {
                if (args[index] == name)
                {
                    return args[index + 1];
                }
            }
            return null;
        }

        private static string RequireCommandLineOption(string name)
        {
            var value = GetCommandLineOption(name);
            if (string.IsNullOrEmpty(value))
            {
                throw new InvalidDataException($"Missing required command-line option {name}.");
            }
            return value;
        }

        private static void WriteReport(string reportPath, MidoriUnityImportReport report)
        {
            var directory = Path.GetDirectoryName(reportPath);
            if (!string.IsNullOrEmpty(directory))
            {
                Directory.CreateDirectory(directory);
            }
            File.WriteAllText(reportPath, JsonUtility.ToJson(report, true));
        }

        private static void EnsureAssetFolder(string assetFolder)
        {
            var parts = assetFolder.Split('/');
            if (parts.Length == 0 || parts[0] != "Assets")
            {
                throw new InvalidDataException("Unity import root must be under Assets/.");
            }

            var current = "Assets";
            for (var index = 1; index < parts.Length; index++)
            {
                var next = $"{current}/{parts[index]}";
                if (!AssetDatabase.IsValidFolder(next))
                {
                    AssetDatabase.CreateFolder(current, parts[index]);
                }
                current = next;
            }
        }

        private static string JoinAssetPath(string root, string relative)
        {
            return $"{root}/{relative}".Replace('\\', '/');
        }

        private static string AssetPathToFullPath(string assetPath)
        {
            return Path.GetFullPath(Path.Combine(Directory.GetCurrentDirectory(), assetPath));
        }

        private static string SanitizeAssetName(string value)
        {
            foreach (var c in Path.GetInvalidFileNameChars())
            {
                value = value.Replace(c, '_');
            }
            return value.Replace(' ', '_');
        }

        private static Vector3 ToVector3(float[] values)
        {
            return new Vector3(values[0], values[1], values[2]);
        }
    }

    public sealed class MidoriNatureScatterAsset : ScriptableObject
    {
        public string packageName;
        public int schemaVersion;
        public int instanceCount;
        public bool recordsValidated;
        [NonSerialized]
        public ulong fileChecksumXor;
        [NonSerialized]
        public ulong recordChecksumXor;
        public List<MidoriScatterChunk> chunks = new List<MidoriScatterChunk>();
        public List<MidoriUnityScatterChunkReport> chunkReports =
            new List<MidoriUnityScatterChunkReport>();
    }

    [Serializable]
    public sealed class MidoriUnityImportReport
    {
        public string assetName;
        public int schemaVersion;
        public string profile;
        public string manifestFileChecksum;
        public int sourceFileCount;
        public string sourceFileChecksumXor;
        public string[] sourceFiles = Array.Empty<string>();
        public string importedAssetPath;
        public string terrainDataAsset;
        public string importScreenshotPath;
        public string densityScreenshotPath;
        public MidoriUnityScreenshotReport importScreenshot;
        public MidoriUnityScreenshotReport densityScreenshot;
        public string heightmapFile;
        public string masksFile;
        public string densityMapFile;
        public string normalMapFile;
        public string unityHintHeightmap;
        public string unityHintDensityMap;
        public string unityHintNormalMap;
        public string unityHintDetailMode;
        public float tileSizeMeters;
        public Vector3 terrainSize;
        public Vector3 terrainPosition;
        public float terrainHeightMin;
        public float terrainHeightMax;
        public int heightmapResolution;
        public int detailResolution;
        public int detailResolutionPerPatch;
        public int prototypesDeclared;
        public int lod0PrototypesDeclared;
        public int lodFilesDeclared;
        public int detailPrototypesCreated;
        public int detailPrototypesLoadedFromAssets;
        public int detailPrototypesGeneratedFromGlb;
        public int detailPrototypeFailures;
        public string[] detailPrototypeGeneratedFiles = Array.Empty<string>();
        public string[] detailPrototypeFallbackErrors = Array.Empty<string>();
        public int nonZeroDetailCells;
        public int scatterBinaryChunks;
        public int scatterBinaryInstances;
        public bool scatterBinaryRecordsValidated;
        public int scatterBinaryRecordsRead;
        public string scatterBinaryFileChecksumXor;
        public string scatterBinaryRecordChecksumXor;
        public MidoriUnityScatterChunkReport[] scatterChunkReports =
            Array.Empty<MidoriUnityScatterChunkReport>();
        public float mobileDensityScale;
        public float mobileLod0MaxDistance;
        public float mobileLod1MaxDistance;
        public float mobileLod2MaxDistance;
        public float mobileCullStartMeters;
        public float mobileCullEndMeters;
        public bool mobileShadows;
        public int mobileMaterialSlots;
        public int mobileMaxInstancesPerTile;
        public int mobileMaxInstancesPerChunk;
        public bool mobileGrassCollision;
        public bool mobileMossCollision;
        public int materialSlotsDeclared;
        public bool hasTerrainSurfaceMaterialSlot;
        public bool hasGroundcoverFoliageMaterialSlot;
        public int materialParameterSetCount;
        public string[] materialParameterSlots = Array.Empty<string>();
        public string[] materialParameterRuntimePolicies = Array.Empty<string>();
        public string[] materialParameterSemantics = Array.Empty<string>();
        public string[] groundcoverMaterialParameterNames = Array.Empty<string>();
        public string[] terrainMaterialParameterNames = Array.Empty<string>();
        public int materialRecipeCount;
        public string[] materialRecipeFiles = Array.Empty<string>();
        public string[] materialRecipeRuntimePolicies = Array.Empty<string>();
        public string[] materialRecipeEngineTargets = Array.Empty<string>();
        public string terrainMaterialRecipeFile;
        public string groundcoverMaterialRecipeFile;
        public int engineImportRecipeCount;
        public string[] engineImportRecipeFiles = Array.Empty<string>();
        public string[] engineImportRecipeProfiles = Array.Empty<string>();
        public string[] engineImportRecipeRuntimePolicies = Array.Empty<string>();
        public string[] engineImportRecipeExpectedSystems = Array.Empty<string>();
        public string unityEngineImportRecipeFile;
        public string unrealEngineImportRecipeFile;
        public string[] prototypeSurfaceTargets = Array.Empty<string>();
        public string[] rockPrototypeSurfaceTargets = Array.Empty<string>();
        public string[] logPrototypeSurfaceTargets = Array.Empty<string>();
        public string[] shrubPrototypeSurfaceTargets = Array.Empty<string>();
        public string groundcoverMaterialAlphaMode;
        public bool groundcoverMaterialDoubleSided;
        public bool groundcoverMaterialShadows;
        public int surfaceOverlayCount;
        public string[] surfaceOverlayNames = Array.Empty<string>();
        public string[] surfaceOverlaySourceFiles = Array.Empty<string>();
        public string[] surfaceOverlayChannels = Array.Empty<string>();
        public string[] surfaceOverlayTargets = Array.Empty<string>();
        public string[] surfaceOverlayRuntimePolicies = Array.Empty<string>();
        public string windPhase;
        public string windStiffness;
        public string windHeight;
        public string windColorVariation;
        public string windNormalizedProgress;
        public string[] notes = Array.Empty<string>();
    }

    [Serializable]
    public sealed class MidoriUnityScreenshotReport
    {
        public string path;
        public string status;
        public bool exists;
        public long bytes;
        public string checksum;
        public int width;
        public int height;
    }

    [Serializable]
    public sealed class MidoriScatterChunk
    {
        public string file;
        public string layerName;
        public string kind;
        public int chunkX;
        public int chunkZ;
        public Vector3 boundsMin;
        public Vector3 boundsMax;
        public List<MidoriScatterInstance> instances = new List<MidoriScatterInstance>();
        public MidoriUnityScatterChunkReport report;
        [NonSerialized]
        public ulong fileChecksumValue;
        [NonSerialized]
        public ulong recordChecksumValue;
    }

    [Serializable]
    public struct MidoriScatterInstance
    {
        public Vector3 position;
        public float yawRadians;
        public float heightMultiplier;
        public float widthMultiplier;
        public float phaseRadians;
        public float colorVariation;
    }

    [Serializable]
    public sealed class MidoriUnityScatterChunkReport
    {
        public string file;
        public string layerName;
        public string kind;
        public int chunkX;
        public int chunkZ;
        public int instanceCount;
        public Vector3 boundsMin;
        public Vector3 boundsMax;
        public Vector3 positionMin;
        public Vector3 positionMax;
        public float yawMin;
        public float yawMax;
        public float heightMin;
        public float heightMax;
        public float widthMin;
        public float widthMax;
        public float phaseMin;
        public float phaseMax;
        public float colorVariationMin;
        public float colorVariationMax;
        public string fileChecksum;
        public string recordChecksum;
    }

    internal sealed class MidoriUnityPrototypeImportSummary
    {
        public int loadedFromAssets;
        public int generatedFromGlb;
        public int failures;
        public List<string> generatedFiles = new List<string>();
        public List<string> errors = new List<string>();
    }

    [Serializable]
    internal sealed class MidoriManifest
    {
        public int schema_version;
        public string asset_name;
        public float tile_size;
        public int map_resolution;
        public MidoriTerrainManifest terrain;
        public MidoriNormalConventions normal_conventions;
        public MidoriPrototype[] prototypes = Array.Empty<MidoriPrototype>();
        public MidoriMaterialSlot[] material_slots = Array.Empty<MidoriMaterialSlot>();
        public MidoriMaterialParameterSet[] material_parameters =
            Array.Empty<MidoriMaterialParameterSet>();
        public MidoriMaterialRecipe[] material_recipes = Array.Empty<MidoriMaterialRecipe>();
        public MidoriEngineImportRecipe[] engine_import_recipes =
            Array.Empty<MidoriEngineImportRecipe>();
        public MidoriSurfaceOverlay[] surface_overlays = Array.Empty<MidoriSurfaceOverlay>();
        public MidoriScatterManifest scatter;
        public MidoriWindPacking wind_packing;
        public MidoriProfile mobile;
        public MidoriUnityHints unity;
        public string shader_policy;
        public string texture_pipeline;
    }

    [Serializable]
    internal sealed class MidoriTerrainManifest
    {
        public string heightmap_file;
        public string masks_file;
        public string grass_density_file;
        public float height_min;
        public float height_max;
        public float[] bounds_min = new float[3];
        public float[] bounds_max = new float[3];
    }

    [Serializable]
    internal sealed class MidoriNormalConventions
    {
        public string unity_yplus_file;
    }

    [Serializable]
    internal sealed class MidoriSurfaceOverlay
    {
        public string name;
        public string source_file;
        public string channel;
        public string[] targets = Array.Empty<string>();
        public string application;
        public string runtime_policy;
    }

    [Serializable]
    internal sealed class MidoriPrototype
    {
        public string name;
        public string kind;
        public string[] surface_targets = Array.Empty<string>();
        public MidoriPrototypeLod[] lods = Array.Empty<MidoriPrototypeLod>();
    }

    [Serializable]
    internal sealed class MidoriPrototypeLod
    {
        public int index;
        public string file;
    }

    [Serializable]
    internal sealed class MidoriMaterialSlot
    {
        public string name;
        public string alpha_mode;
        public bool double_sided;
        public bool shadows;
    }

    [Serializable]
    internal sealed class MidoriMaterialParameterSet
    {
        public string material_slot;
        public string parameter_set;
        public string runtime_policy;
        public MidoriMaterialParameter[] parameters = Array.Empty<MidoriMaterialParameter>();
    }

    [Serializable]
    internal sealed class MidoriMaterialParameter
    {
        public string name;
        public string semantic;
        public string value_type;
        public string source;
        public string default_value;
    }

    [Serializable]
    internal sealed class MidoriMaterialRecipe
    {
        public string material_slot;
        public string file;
        public string runtime_policy;
        public string[] engine_targets = Array.Empty<string>();
    }

    [Serializable]
    internal sealed class MidoriEngineImportRecipe
    {
        public string engine;
        public string file;
        public string profile;
        public string runtime_policy;
        public string[] expected_systems = Array.Empty<string>();
    }

    [Serializable]
    internal sealed class MidoriScatterManifest
    {
        public string file;
        public MidoriScatterBinaryFile[] binary_files = Array.Empty<MidoriScatterBinaryFile>();
        public MidoriScatterBinaryFormat binary_format;
    }

    [Serializable]
    internal sealed class MidoriScatterBinaryFile
    {
        public string layer_name;
        public string kind;
        public int chunk_x;
        public int chunk_z;
        public string file;
        public uint instance_count;
        public float[] bounds_min = new float[3];
        public float[] bounds_max = new float[3];
    }

    [Serializable]
    internal sealed class MidoriScatterBinaryFormat
    {
        public string format;
        public int header_bytes;
        public int record_stride_bytes;
        public string endian;
    }

    [Serializable]
    internal sealed class MidoriWindPacking
    {
        public string phase;
        public string stiffness;
        public string height;
        public string color_variation;
        public string normalized_progress;
    }

    [Serializable]
    internal sealed class MidoriUnityHints
    {
        public string terrain_heightmap;
        public string detail_density_map;
        public string normal_map;
        public string detail_mode;
    }

    [Serializable]
    internal sealed class MidoriProfile
    {
        public float density_scale = 1.0f;
        public float lod0_max_distance;
        public float lod1_max_distance;
        public float lod2_max_distance;
        public float cull_start;
        public float cull_end;
        public bool shadows;
        public int material_slots;
        public int max_instances_per_tile;
        public int max_instances_per_chunk;
        public bool grass_collision;
        public bool moss_collision;
    }

    [Serializable]
    internal sealed class MidoriGltfRoot
    {
        public MidoriGltfMesh[] meshes = Array.Empty<MidoriGltfMesh>();
        public MidoriGltfBufferView[] bufferViews = Array.Empty<MidoriGltfBufferView>();
        public MidoriGltfAccessor[] accessors = Array.Empty<MidoriGltfAccessor>();
    }

    [Serializable]
    internal sealed class MidoriGltfMesh
    {
        public MidoriGltfPrimitive[] primitives = Array.Empty<MidoriGltfPrimitive>();
    }

    [Serializable]
    internal sealed class MidoriGltfPrimitive
    {
        public MidoriGltfAttributes attributes;
        public int indices = -1;
    }

    [Serializable]
    internal sealed class MidoriGltfAttributes
    {
        public int POSITION = -1;
        public int NORMAL = -1;
        public int TANGENT = -1;
        public int TEXCOORD_0 = -1;
        public int TEXCOORD_1 = -1;
        public int COLOR_0 = -1;
    }

    [Serializable]
    internal sealed class MidoriGltfBufferView
    {
        public int buffer = 0;
        public int byteOffset = 0;
        public int byteLength = 0;
        public int byteStride = 0;
    }

    [Serializable]
    internal sealed class MidoriGltfAccessor
    {
        public int bufferView = -1;
        public int byteOffset = 0;
        public int componentType = 0;
        public int count = 0;
        public string type;
    }
}
#endif
