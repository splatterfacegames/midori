#!/usr/bin/env python3
"""Compile and exercise the Unity importer against small Unity API stubs.

This is an editorless syntax/API-shape guard. It does not replace a licensed
Unity editor import, but it catches C# mistakes in the importer before the
script is handed to an editor machine. When a package directory is supplied it
also executes the importer's pure manifest/source/scatter validation paths.
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path


CS_PROJ = """<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <OutputType>Exe</OutputType>
    <DefineConstants>UNITY_EDITOR</DefineConstants>
    <ImplicitUsings>false</ImplicitUsings>
    <Nullable>disable</Nullable>
    <NoWarn>CS0169;CS0649</NoWarn>
  </PropertyGroup>
</Project>
"""


UNITY_STUBS = r"""
using System;
using System.Collections.Generic;
using System.Text;
using System.Text.Json;

namespace UnityEngine
{
    public class Object
    {
        public static void DestroyImmediate(Object obj) {}
    }

    public class ScriptableObject : Object
    {
        public static T CreateInstance<T>() where T : ScriptableObject, new() => new T();
    }

    public struct Vector2
    {
        public float x;
        public float y;
        public Vector2(float x, float y) { this.x = x; this.y = y; }
    }

    public struct Vector3
    {
        public float x;
        public float y;
        public float z;
        public Vector3(float x, float y, float z) { this.x = x; this.y = y; this.z = z; }
        public static Vector3 Min(Vector3 a, Vector3 b) => new Vector3(Mathf.Min(a.x, b.x), Mathf.Min(a.y, b.y), Mathf.Min(a.z, b.z));
        public static Vector3 Max(Vector3 a, Vector3 b) => new Vector3(Mathf.Max(a.x, b.x), Mathf.Max(a.y, b.y), Mathf.Max(a.z, b.z));
    }

    public struct Vector4
    {
        public float x;
        public float y;
        public float z;
        public float w;
        public Vector4(float x, float y, float z, float w) { this.x = x; this.y = y; this.z = z; this.w = w; }
    }

    public struct Quaternion
    {
        public static Quaternion Euler(float x, float y, float z) => new Quaternion();
    }

    public struct Color
    {
        public float r;
        public float g;
        public float b;
        public float a;
        public Color(float r, float g, float b, float a = 1.0f) { this.r = r; this.g = g; this.b = b; this.a = a; }
        public float grayscale => (r + g + b) / 3.0f;
        public static Color white => new Color(1.0f, 1.0f, 1.0f, 1.0f);
    }

    public static class Mathf
    {
        public static float Max(float a, float b) => Math.Max(a, b);
        public static int Max(int a, int b) => Math.Max(a, b);
        public static float Min(float a, float b) => Math.Min(a, b);
        public static int RoundToInt(float value) => (int)Math.Round(value);
        public static float Clamp01(float value) => Math.Max(0.0f, Math.Min(1.0f, value));
        public static int NextPowerOfTwo(int value)
        {
            var result = 1;
            while (result < value) result <<= 1;
            return result;
        }
    }

    public class Transform
    {
        public Vector3 position;
        public Quaternion rotation;
    }

    public class GameObject : Object
    {
        public string name;
        public Transform transform = new Transform();
        public GameObject() {}
        public GameObject(string name) { this.name = name; }
        public T AddComponent<T>() where T : new() => new T();
    }

    public static class Terrain
    {
        public static GameObject CreateTerrainGameObject(TerrainData data) => new GameObject("Terrain");
    }

    public class TerrainData : Object
    {
        private readonly Dictionary<int, int[,]> detailLayers = new Dictionary<int, int[,]>();
        public int heightmapResolution;
        public Vector3 size;
        public int detailResolution;
        public DetailPrototype[] detailPrototypes = Array.Empty<DetailPrototype>();
        public void SetHeights(int xBase, int yBase, float[,] heights) {}
        public void SetDetailResolution(int detailResolution, int resolutionPerPatch) { this.detailResolution = detailResolution; }
        public void RefreshPrototypes() {}
        public int[,] GetDetailLayer(int xBase, int yBase, int width, int height, int layer)
        {
            if (detailLayers.TryGetValue(layer, out var details))
            {
                return details;
            }
            return new int[height, width];
        }
        public void SetDetailLayer(int xBase, int yBase, int layer, int[,] details)
        {
            detailLayers[layer] = details;
        }
    }

    public class DetailPrototype
    {
        public GameObject prototype;
        public bool usePrototypeMesh;
        public bool useInstancing;
        public DetailRenderMode renderMode;
        public float minWidth;
        public float maxWidth;
        public float minHeight;
        public float maxHeight;
        public Color healthyColor;
        public Color dryColor;
    }

    public enum DetailRenderMode { VertexLit }

    public class Camera : Object
    {
        public bool orthographic;
        public float orthographicSize;
        public float nearClipPlane;
        public float farClipPlane;
        public CameraClearFlags clearFlags;
        public Color backgroundColor;
        public RenderTexture targetTexture;
        public void Render() {}
    }

    public enum CameraClearFlags { SolidColor }

    public class RenderTexture : Object
    {
        public static RenderTexture active;
        public RenderTexture(int width, int height, int depth, RenderTextureFormat format) {}
        public void Release() {}
    }

    public enum RenderTextureFormat { ARGB32 }
    public enum TextureFormat { RGBA32 }

    public struct Rect
    {
        public float x;
        public float y;
        public float width;
        public float height;
        public Rect(float x, float y, float width, float height) { this.x = x; this.y = y; this.width = width; this.height = height; }
    }

    public class Texture2D : Object
    {
        private readonly string assetPath;
        private readonly int width;
        private readonly int height;
        private int pixelCount;
        private float minR = float.PositiveInfinity;
        private float minG = float.PositiveInfinity;
        private float minB = float.PositiveInfinity;
        private float maxR = float.NegativeInfinity;
        private float maxG = float.NegativeInfinity;
        private float maxB = float.NegativeInfinity;
        public static int LastEncodedWidth;
        public static int LastEncodedHeight;
        public static int LastEncodedPixelCount;
        public static float LastEncodedMinR;
        public static float LastEncodedMaxR;
        public static float LastEncodedMinG;
        public static float LastEncodedMaxG;
        public static float LastEncodedMinB;
        public static float LastEncodedMaxB;
        public Texture2D(int width, int height, TextureFormat format, bool mipChain) { this.width = width; this.height = height; }
        private Texture2D(string assetPath) { this.assetPath = assetPath; }
        public static Texture2D ForAssetPath(string assetPath) => new Texture2D(assetPath);
        public Color GetPixelBilinear(float u, float v)
        {
            var wave = 0.5f + 0.25f * (float)Math.Sin((u * 17.0f) + (v * 11.0f));
            if (assetPath != null && assetPath.Contains("masks_rgba"))
            {
                return new Color(0.35f, Math.Max(0.1f, wave), 0.3f, Math.Max(0.05f, wave));
            }
            if (assetPath != null && assetPath.Contains("grass_density"))
            {
                return new Color(Math.Max(0.1f, wave), Math.Max(0.1f, wave), Math.Max(0.1f, wave), 1.0f);
            }
            return new Color(Math.Max(0.0f, Math.Min(1.0f, wave)), Math.Max(0.0f, Math.Min(1.0f, wave)), Math.Max(0.0f, Math.Min(1.0f, wave)), 1.0f);
        }
        public void SetPixel(int x, int y, Color color)
        {
            pixelCount++;
            minR = Math.Min(minR, color.r);
            minG = Math.Min(minG, color.g);
            minB = Math.Min(minB, color.b);
            maxR = Math.Max(maxR, color.r);
            maxG = Math.Max(maxG, color.g);
            maxB = Math.Max(maxB, color.b);
        }
        public void Apply() {}
        public byte[] EncodeToPNG()
        {
            LastEncodedWidth = width;
            LastEncodedHeight = height;
            LastEncodedPixelCount = pixelCount;
            LastEncodedMinR = pixelCount > 0 ? minR : 0.0f;
            LastEncodedMaxR = pixelCount > 0 ? maxR : 0.0f;
            LastEncodedMinG = pixelCount > 0 ? minG : 0.0f;
            LastEncodedMaxG = pixelCount > 0 ? maxG : 0.0f;
            LastEncodedMinB = pixelCount > 0 ? minB : 0.0f;
            LastEncodedMaxB = pixelCount > 0 ? maxB : 0.0f;
            return Encoding.UTF8.GetBytes(
                $"MIDORI_STUB_PNG {width} {height} {pixelCount} {LastEncodedMinR} {LastEncodedMaxR} {LastEncodedMinG} {LastEncodedMaxG} {LastEncodedMinB} {LastEncodedMaxB}"
            );
        }
        public void ReadPixels(Rect source, int destX, int destY)
        {
            pixelCount = Math.Max(1, (int)Math.Round(source.width * source.height));
            minR = 0.08f;
            maxR = 0.72f;
            minG = 0.10f;
            maxG = 0.82f;
            minB = 0.08f;
            maxB = 0.35f;
        }
    }

    public class Mesh : Object
    {
        public string name;
        public Rendering.IndexFormat indexFormat;
        public void SetVertices(List<Vector3> vertices) {}
        public void SetNormals(List<Vector3> normals) {}
        public void SetUVs(int channel, List<Vector2> uvs) {}
        public void SetTangents(List<Vector4> tangents) {}
        public void SetColors(List<Color> colors) {}
        public void SetTriangles(List<int> triangles, int submesh) {}
        public void RecalculateNormals() {}
        public void RecalculateBounds() {}
    }

    public class Shader : Object
    {
        public static Shader Find(string name) => new Shader();
    }

    public class Material : Object
    {
        public string name;
        public Color color;
        public Material(Shader shader) {}
    }

    public class MeshFilter
    {
        public Mesh sharedMesh;
    }

    public class MeshRenderer
    {
        public Material sharedMaterial;
    }

    public static class Debug
    {
        public static void Log(object message) {}
        public static void LogWarning(object message) {}
        public static void LogException(Exception exception) {}
    }

    public static class JsonUtility
    {
        private static JsonSerializerOptions Options(bool prettyPrint = false) => new JsonSerializerOptions
        {
            IncludeFields = true,
            PropertyNameCaseInsensitive = false,
            WriteIndented = prettyPrint
        };

        public static T FromJson<T>(string json) where T : new()
            => JsonSerializer.Deserialize<T>(json, Options()) ?? new T();

        public static string ToJson(object obj, bool prettyPrint)
            => JsonSerializer.Serialize(obj, Options(prettyPrint));
    }
}

namespace UnityEngine.Rendering
{
    public enum IndexFormat { UInt16, UInt32 }
}

namespace UnityEditor
{
    using UnityEngine;

    public sealed class MenuItemAttribute : Attribute
    {
        public MenuItemAttribute(string itemName) {}
    }

    public static class EditorUtility
    {
        public static string OpenFolderPanel(string title, string folder, string defaultName) => "";
    }

    public static class EditorApplication
    {
        public static void Exit(int exitCode) {}
    }

    public static class AssetDatabase
    {
        private static readonly Dictionary<string, Object> assets = new Dictionary<string, Object>();
        public static string GenerateUniqueAssetPath(string path) => path;
        public static void Refresh() {}
        public static void CreateAsset(Object asset, string path) { assets[path] = asset; }
        public static void SaveAssets() {}
        public static T LoadAssetAtPath<T>(string path) where T : class
        {
            if (assets.TryGetValue(path, out var asset))
            {
                return asset as T;
            }
            if (typeof(T) == typeof(Texture2D)
                && (path.EndsWith(".png", StringComparison.OrdinalIgnoreCase)
                    || path.EndsWith(".jpg", StringComparison.OrdinalIgnoreCase)
                    || path.EndsWith(".jpeg", StringComparison.OrdinalIgnoreCase)))
            {
                return Texture2D.ForAssetPath(path) as T;
            }
            return null;
        }
        public static bool IsValidFolder(string path) => true;
        public static void CreateFolder(string parentFolder, string newFolderName) {}
    }

    public static class FileUtil
    {
        public static void CopyFileOrDirectory(string source, string destination) {}
    }

    public static class Selection
    {
        public static Object activeObject;
    }

    public class AssetImporter
    {
        public static AssetImporter GetAtPath(string assetPath) => null;
    }

    public class TextureImporter : AssetImporter
    {
        public TextureImporterType textureType;
        public bool sRGBTexture;
        public bool isReadable;
        public TextureImporterCompression textureCompression;
        public void SaveAndReimport() {}
    }

    public enum TextureImporterType { Default, NormalMap }
    public enum TextureImporterCompression { Uncompressed }

    public static class PrefabUtility
    {
        public static GameObject SaveAsPrefabAsset(GameObject instanceRoot, string assetPath) => instanceRoot;
    }
}
"""


HARNESS = r"""
using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text.Json;
using Midori.Unity;
using UnityEngine;

internal static class MidoriUnityImporterExecutionHarness
{
    private static readonly Type ImporterType = typeof(MidoriNaturePackageImporter);
    private static readonly BindingFlags PrivateStatic =
        BindingFlags.NonPublic | BindingFlags.Static;

    public static int Main(string[] args)
    {
        try
        {
            if (args.Length < 2)
            {
                Console.WriteLine("unity importer compile stub ok");
                return 0;
            }

            var packageDirectory = Path.GetFullPath(args[0]);
            var reportPath = Path.GetFullPath(args[1]);
            if (!Directory.Exists(packageDirectory))
            {
                throw new DirectoryNotFoundException(packageDirectory);
            }

            var manifest = Invoke("ReadManifest", Path.Combine(packageDirectory, "midori_nature.json"));
            Invoke("ValidateManifest", manifest);

            var sourceFiles = (string[])Invoke("CollectUnitySourceFiles", manifest);
            Invoke("ValidateSourceFiles", packageDirectory, sourceFiles);

            var manifestChecksum = (ulong)Invoke(
                "FileChecksum",
                Path.Combine(packageDirectory, "midori_nature.json")
            );
            var sourceChecksumXor = (ulong)Invoke(
                "SourceFileChecksumXor",
                packageDirectory,
                sourceFiles
            );

            var scatter = (MidoriNatureScatterAsset)Invoke(
                "CreateScatterAsset",
                packageDirectory,
                manifest
            );
            var fakePackageAssetPath = "Assets/MidoriFake/forest_floor";
            var fakePackageDirectory = Path.GetFullPath(fakePackageAssetPath);
            if (Directory.Exists(fakePackageDirectory))
            {
                Directory.Delete(fakePackageDirectory, true);
            }
            CopyDirectory(packageDirectory, fakePackageDirectory);
            var terrainArgs = new object[] { fakePackageAssetPath, manifest, null };
            var terrainData = (TerrainData)Invoke("CreateTerrainData", terrainArgs);
            var prototypeImportSummary = terrainArgs[2];
            var reportDirectory = Path.GetDirectoryName(reportPath);
            var terrainScreenshotPath = Path.Combine(
                string.IsNullOrEmpty(reportDirectory) ? "." : reportDirectory,
                "unity_import_screenshot_compile_stub.png"
            );
            var terrainObject = Terrain.CreateTerrainGameObject(terrainData);
            Invoke("WriteTerrainScreenshot", terrainObject, terrainData, terrainScreenshotPath);
            var terrainScreenshotBytes = File.ReadAllBytes(terrainScreenshotPath);
            var terrainScreenshotWidth = Texture2D.LastEncodedWidth;
            var terrainScreenshotHeight = Texture2D.LastEncodedHeight;
            var terrainScreenshotPixelCount = Texture2D.LastEncodedPixelCount;
            var terrainScreenshotMinR = Texture2D.LastEncodedMinR;
            var terrainScreenshotMaxR = Texture2D.LastEncodedMaxR;
            var terrainScreenshotMinG = Texture2D.LastEncodedMinG;
            var terrainScreenshotMaxG = Texture2D.LastEncodedMaxG;
            var terrainScreenshotMinB = Texture2D.LastEncodedMinB;
            var terrainScreenshotMaxB = Texture2D.LastEncodedMaxB;
            var densityScreenshotPath = Path.Combine(
                string.IsNullOrEmpty(reportDirectory) ? "." : reportDirectory,
                "unity_density_screenshot_compile_stub.png"
            );
            Invoke("WriteDensityScreenshot", fakePackageAssetPath, manifest, densityScreenshotPath);
            var densityScreenshotBytes = File.ReadAllBytes(densityScreenshotPath);

            var report = new HarnessReport
            {
                status = "passed",
                package_dir = packageDirectory,
                asset_name = Field<string>(manifest, "asset_name"),
                schema_version = Field<int>(manifest, "schema_version"),
                manifest_file_checksum = ToHex(manifestChecksum),
                source_file_count = sourceFiles.Length,
                source_file_checksum_xor = ToHex(sourceChecksumXor),
                terrain_size_x = terrainData.size.x,
                terrain_size_y = terrainData.size.y,
                terrain_size_z = terrainData.size.z,
                heightmapResolution = terrainData.heightmapResolution,
                detailResolution = terrainData.detailResolution,
                detailResolutionPerPatch = 8,
                detailPrototypesCreated = terrainData.detailPrototypes.Length,
                detailPrototypesLoadedFromAssets = Field<int>(
                    prototypeImportSummary,
                    "loadedFromAssets"
                ),
                detailPrototypesGeneratedFromGlb = Field<int>(
                    prototypeImportSummary,
                    "generatedFromGlb"
                ),
                detailPrototypeFailures = Field<int>(prototypeImportSummary, "failures"),
                detailPrototypeGeneratedFileCount = ListCount(
                    prototypeImportSummary,
                    "generatedFiles"
                ),
                detailPrototypeFallbackErrorCount = ListCount(prototypeImportSummary, "errors"),
                nonZeroDetailCells = (int)Invoke("CountNonZeroDetailCells", terrainData),
                importScreenshotPath = terrainScreenshotPath,
                importScreenshotExists = File.Exists(terrainScreenshotPath),
                importScreenshotBytes = terrainScreenshotBytes.Length,
                importScreenshotChecksum = ToHex(Fnv1a(terrainScreenshotBytes)),
                importScreenshotWidth = terrainScreenshotWidth,
                importScreenshotHeight = terrainScreenshotHeight,
                importScreenshotPixelCount = terrainScreenshotPixelCount,
                importScreenshotMinR = terrainScreenshotMinR,
                importScreenshotMaxR = terrainScreenshotMaxR,
                importScreenshotMinG = terrainScreenshotMinG,
                importScreenshotMaxG = terrainScreenshotMaxG,
                importScreenshotMinB = terrainScreenshotMinB,
                importScreenshotMaxB = terrainScreenshotMaxB,
                densityScreenshotPath = densityScreenshotPath,
                densityScreenshotExists = File.Exists(densityScreenshotPath),
                densityScreenshotBytes = densityScreenshotBytes.Length,
                densityScreenshotChecksum = ToHex(Fnv1a(densityScreenshotBytes)),
                densityScreenshotWidth = Texture2D.LastEncodedWidth,
                densityScreenshotHeight = Texture2D.LastEncodedHeight,
                densityScreenshotPixelCount = Texture2D.LastEncodedPixelCount,
                densityScreenshotMinR = Texture2D.LastEncodedMinR,
                densityScreenshotMaxR = Texture2D.LastEncodedMaxR,
                densityScreenshotMinG = Texture2D.LastEncodedMinG,
                densityScreenshotMaxG = Texture2D.LastEncodedMaxG,
                densityScreenshotMinB = Texture2D.LastEncodedMinB,
                densityScreenshotMaxB = Texture2D.LastEncodedMaxB,
                prototypesDeclared = ArrayLength(manifest, "prototypes"),
                lod0PrototypesDeclared = (int)Invoke("CountLod0Prototypes", manifest),
                lodFilesDeclared = (int)Invoke("CountLodFiles", manifest),
                materialSlotsDeclared = ArrayLength(manifest, "material_slots"),
                hasTerrainSurfaceMaterialSlot = (bool)Invoke(
                    "HasMaterialSlot",
                    manifest,
                    "terrain_surface"
                ),
                hasGroundcoverFoliageMaterialSlot = (bool)Invoke(
                    "HasMaterialSlot",
                    manifest,
                    "groundcover_foliage"
                ),
                groundcoverMaterialAlphaMode = (string)Invoke(
                    "MaterialSlotValue",
                    manifest,
                    "groundcover_foliage",
                    new Func<MidoriMaterialSlot, string>(slot => slot.alpha_mode)
                ),
                groundcoverMaterialDoubleSided = (bool)Invoke(
                    "MaterialSlotBool",
                    manifest,
                    "groundcover_foliage",
                    new Func<MidoriMaterialSlot, bool>(slot => slot.double_sided)
                ),
                groundcoverMaterialShadows = (bool)Invoke(
                    "MaterialSlotBool",
                    manifest,
                    "groundcover_foliage",
                    new Func<MidoriMaterialSlot, bool>(slot => slot.shadows)
                ),
                materialParameterSetCount = ArrayLength(manifest, "material_parameters"),
                materialParameterSlots = InvokeStringArray("MaterialParameterSlots", manifest),
                materialParameterRuntimePolicies = InvokeStringArray(
                    "MaterialParameterRuntimePolicies",
                    manifest
                ),
                materialParameterSemantics = InvokeStringArray(
                    "MaterialParameterSemantics",
                    manifest
                ),
                groundcoverMaterialParameterNames = InvokeStringArray(
                    "MaterialParameterNamesForSlot",
                    manifest,
                    "groundcover_foliage"
                ),
                terrainMaterialParameterNames = InvokeStringArray(
                    "MaterialParameterNamesForSlot",
                    manifest,
                    "terrain_surface"
                ),
                material_recipe_count = ArrayLength(manifest, "material_recipes"),
                materialRecipeCount = ArrayLength(manifest, "material_recipes"),
                materialRecipeFiles = InvokeStringArray("MaterialRecipeFiles", manifest),
                materialRecipeRuntimePolicies = InvokeStringArray(
                    "MaterialRecipeRuntimePolicies",
                    manifest
                ),
                materialRecipeEngineTargets = InvokeStringArray(
                    "MaterialRecipeEngineTargets",
                    manifest
                ),
                terrainMaterialRecipeFile = (string)Invoke(
                    "MaterialRecipeFileForSlot",
                    manifest,
                    "terrain_surface"
                ),
                groundcoverMaterialRecipeFile = (string)Invoke(
                    "MaterialRecipeFileForSlot",
                    manifest,
                    "groundcover_foliage"
                ),
                engine_import_recipe_count = ArrayLength(manifest, "engine_import_recipes"),
                engine_import_recipe_files = StringFields(manifest, "engine_import_recipes", "file"),
                engineImportRecipeCount = ArrayLength(manifest, "engine_import_recipes"),
                engineImportRecipeFiles = InvokeStringArray("EngineImportRecipeFiles", manifest),
                engineImportRecipeProfiles = InvokeStringArray(
                    "EngineImportRecipeProfiles",
                    manifest
                ),
                engineImportRecipeRuntimePolicies = InvokeStringArray(
                    "EngineImportRecipeRuntimePolicies",
                    manifest
                ),
                engineImportRecipeExpectedSystems = InvokeStringArray(
                    "EngineImportRecipeExpectedSystems",
                    manifest
                ),
                unityEngineImportRecipeFile = (string)Invoke(
                    "EngineImportRecipeFileForEngine",
                    manifest,
                    "unity"
                ),
                unrealEngineImportRecipeFile = (string)Invoke(
                    "EngineImportRecipeFileForEngine",
                    manifest,
                    "unreal"
                ),
                surface_overlay_count = ArrayLength(manifest, "surface_overlays"),
                surfaceOverlayCount = ArrayLength(manifest, "surface_overlays"),
                surfaceOverlayNames = InvokeStringArray("SurfaceOverlayNames", manifest),
                surfaceOverlaySourceFiles = InvokeStringArray(
                    "SurfaceOverlaySourceFiles",
                    manifest
                ),
                surfaceOverlayChannels = InvokeStringArray("SurfaceOverlayChannels", manifest),
                surfaceOverlayTargets = InvokeStringArray("SurfaceOverlayTargets", manifest),
                surfaceOverlayRuntimePolicies = InvokeStringArray(
                    "SurfaceOverlayRuntimePolicies",
                    manifest
                ),
                prototype_count = ArrayLength(manifest, "prototypes"),
                prototypeSurfaceTargets = InvokeStringArray("PrototypeSurfaceTargets", manifest),
                rockPrototypeSurfaceTargets = InvokeStringArray(
                    "PrototypeSurfaceTargetsForKind",
                    manifest,
                    "rock"
                ),
                logPrototypeSurfaceTargets = InvokeStringArray(
                    "PrototypeSurfaceTargetsForKind",
                    manifest,
                    "log"
                ),
                shrubPrototypeSurfaceTargets = InvokeStringArray(
                    "PrototypeSurfaceTargetsForKind",
                    manifest,
                    "shrub"
                ),
                scatter_binary_chunks = scatter.chunks.Count,
                scatter_chunk_reports = scatter.chunkReports.Count,
                scatter_binary_instances = scatter.instanceCount,
                scatter_binary_records_validated = scatter.recordsValidated,
                scatter_binary_file_checksum_xor = ToHex(scatter.fileChecksumXor),
                scatter_binary_record_checksum_xor = ToHex(scatter.recordChecksumXor)
            };

            var directory = Path.GetDirectoryName(reportPath);
            if (!string.IsNullOrEmpty(directory))
            {
                Directory.CreateDirectory(directory);
            }
            File.WriteAllText(
                reportPath,
                JsonSerializer.Serialize(
                    report,
                    new JsonSerializerOptions { IncludeFields = true, WriteIndented = true }
                )
            );
            Console.WriteLine($"unity importer execution stub ok: {reportPath}");
            return 0;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine(Unwrap(exception));
            return 1;
        }
    }

    private static object Invoke(string name, params object[] args)
    {
        var method = ImporterType.GetMethod(name, PrivateStatic);
        if (method == null)
        {
            throw new MissingMethodException(ImporterType.FullName, name);
        }
        return method.Invoke(null, args);
    }

    private static string[] InvokeStringArray(string name, params object[] args)
    {
        return (string[])Invoke(name, args);
    }

    private static int ListCount(object target, string name)
    {
        var field = target.GetType().GetField(name);
        if (field == null)
        {
            throw new MissingFieldException(target.GetType().FullName, name);
        }
        return ((System.Collections.ICollection)field.GetValue(target)).Count;
    }

    private static T Field<T>(object target, string name)
    {
        var field = target.GetType().GetField(name);
        if (field == null)
        {
            throw new MissingFieldException(target.GetType().FullName, name);
        }
        return (T)field.GetValue(target);
    }

    private static int ArrayLength(object target, string name)
    {
        return Field<Array>(target, name)?.Length ?? 0;
    }

    private static string[] StringFields(object target, string arrayFieldName, string itemFieldName)
    {
        var array = Field<Array>(target, arrayFieldName);
        if (array == null)
        {
            return Array.Empty<string>();
        }

        var values = new List<string>();
        foreach (var item in array)
        {
            var field = item.GetType().GetField(itemFieldName);
            values.Add((string)(field?.GetValue(item) ?? ""));
        }
        return values.ToArray();
    }

    private static string ToHex(ulong value) => $"0x{value:x16}";

    private static ulong Fnv1a(byte[] data)
    {
        const ulong offsetBasis = 0xcbf29ce484222325UL;
        const ulong prime = 0x100000001b3UL;
        var value = offsetBasis;
        foreach (var item in data)
        {
            value ^= item;
            value *= prime;
        }
        return value;
    }

    private static void CopyDirectory(string sourceDirectory, string destinationDirectory)
    {
        Directory.CreateDirectory(destinationDirectory);
        foreach (var file in Directory.GetFiles(sourceDirectory))
        {
            File.Copy(file, Path.Combine(destinationDirectory, Path.GetFileName(file)), true);
        }
        foreach (var directory in Directory.GetDirectories(sourceDirectory))
        {
            CopyDirectory(
                directory,
                Path.Combine(destinationDirectory, Path.GetFileName(directory))
            );
        }
    }

    private static string Unwrap(Exception exception)
    {
        while (exception is TargetInvocationException && exception.InnerException != null)
        {
            exception = exception.InnerException;
        }
        return exception.ToString();
    }

    public sealed class HarnessReport
    {
        public string status;
        public string package_dir;
        public string asset_name;
        public int schema_version;
        public string manifest_file_checksum;
        public int source_file_count;
        public string source_file_checksum_xor;
        public float terrain_size_x;
        public float terrain_size_y;
        public float terrain_size_z;
        public int heightmapResolution;
        public int detailResolution;
        public int detailResolutionPerPatch;
        public int detailPrototypesCreated;
        public int detailPrototypesLoadedFromAssets;
        public int detailPrototypesGeneratedFromGlb;
        public int detailPrototypeFailures;
        public int detailPrototypeGeneratedFileCount;
        public int detailPrototypeFallbackErrorCount;
        public int nonZeroDetailCells;
        public string importScreenshotPath;
        public bool importScreenshotExists;
        public int importScreenshotBytes;
        public string importScreenshotChecksum;
        public int importScreenshotWidth;
        public int importScreenshotHeight;
        public int importScreenshotPixelCount;
        public float importScreenshotMinR;
        public float importScreenshotMaxR;
        public float importScreenshotMinG;
        public float importScreenshotMaxG;
        public float importScreenshotMinB;
        public float importScreenshotMaxB;
        public string densityScreenshotPath;
        public bool densityScreenshotExists;
        public int densityScreenshotBytes;
        public string densityScreenshotChecksum;
        public int densityScreenshotWidth;
        public int densityScreenshotHeight;
        public int densityScreenshotPixelCount;
        public float densityScreenshotMinR;
        public float densityScreenshotMaxR;
        public float densityScreenshotMinG;
        public float densityScreenshotMaxG;
        public float densityScreenshotMinB;
        public float densityScreenshotMaxB;
        public int prototypesDeclared;
        public int lod0PrototypesDeclared;
        public int lodFilesDeclared;
        public int materialSlotsDeclared;
        public bool hasTerrainSurfaceMaterialSlot;
        public bool hasGroundcoverFoliageMaterialSlot;
        public string groundcoverMaterialAlphaMode;
        public bool groundcoverMaterialDoubleSided;
        public bool groundcoverMaterialShadows;
        public int materialParameterSetCount;
        public string[] materialParameterSlots = Array.Empty<string>();
        public string[] materialParameterRuntimePolicies = Array.Empty<string>();
        public string[] materialParameterSemantics = Array.Empty<string>();
        public string[] groundcoverMaterialParameterNames = Array.Empty<string>();
        public string[] terrainMaterialParameterNames = Array.Empty<string>();
        public int material_recipe_count;
        public int materialRecipeCount;
        public string[] materialRecipeFiles = Array.Empty<string>();
        public string[] materialRecipeRuntimePolicies = Array.Empty<string>();
        public string[] materialRecipeEngineTargets = Array.Empty<string>();
        public string terrainMaterialRecipeFile;
        public string groundcoverMaterialRecipeFile;
        public int engine_import_recipe_count;
        public string[] engine_import_recipe_files = Array.Empty<string>();
        public int engineImportRecipeCount;
        public string[] engineImportRecipeFiles = Array.Empty<string>();
        public string[] engineImportRecipeProfiles = Array.Empty<string>();
        public string[] engineImportRecipeRuntimePolicies = Array.Empty<string>();
        public string[] engineImportRecipeExpectedSystems = Array.Empty<string>();
        public string unityEngineImportRecipeFile;
        public string unrealEngineImportRecipeFile;
        public int surface_overlay_count;
        public int surfaceOverlayCount;
        public string[] surfaceOverlayNames = Array.Empty<string>();
        public string[] surfaceOverlaySourceFiles = Array.Empty<string>();
        public string[] surfaceOverlayChannels = Array.Empty<string>();
        public string[] surfaceOverlayTargets = Array.Empty<string>();
        public string[] surfaceOverlayRuntimePolicies = Array.Empty<string>();
        public int prototype_count;
        public string[] prototypeSurfaceTargets = Array.Empty<string>();
        public string[] rockPrototypeSurfaceTargets = Array.Empty<string>();
        public string[] logPrototypeSurfaceTargets = Array.Empty<string>();
        public string[] shrubPrototypeSurfaceTargets = Array.Empty<string>();
        public int scatter_binary_chunks;
        public int scatter_chunk_reports;
        public int scatter_binary_instances;
        public bool scatter_binary_records_validated;
        public string scatter_binary_file_checksum_xor;
        public string scatter_binary_record_checksum_xor;
    }
}
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source",
        type=Path,
        default=Path("integrations/unity/Editor/MidoriNaturePackageImporter.cs"),
    )
    parser.add_argument(
        "--work-dir",
        type=Path,
        default=Path("target/unity_importer_compile_stub"),
    )
    parser.add_argument(
        "--package-dir",
        type=Path,
        default=None,
        help="Optional Midori package directory to validate through the compiled C# importer paths.",
    )
    parser.add_argument(
        "--execution-report",
        type=Path,
        default=None,
        help="Optional JSON report path for the C# execution harness.",
    )
    args = parser.parse_args()

    dotnet = shutil.which("dotnet")
    if dotnet is None:
        raise AssertionError("dotnet SDK is required for the Unity importer compile stub")

    source = args.source.resolve()
    if not source.is_file():
        raise AssertionError(f"Unity importer source is missing: {source}")

    work_dir = args.work_dir.resolve()
    if work_dir.exists():
        shutil.rmtree(work_dir)
    work_dir.mkdir(parents=True)

    (work_dir / "MidoriUnityImporterCompileStub.csproj").write_text(CS_PROJ, encoding="utf-8")
    (work_dir / "UnityStubs.cs").write_text(UNITY_STUBS, encoding="utf-8")
    (work_dir / "ExecutionHarness.cs").write_text(HARNESS, encoding="utf-8")
    shutil.copy2(source, work_dir / "MidoriNaturePackageImporter.cs")

    result = subprocess.run(
        [dotnet, "build", str(work_dir / "MidoriUnityImporterCompileStub.csproj"), "--nologo", "--verbosity", "minimal"],
        cwd=work_dir,
        text=True,
        capture_output=True,
    )
    (work_dir / "dotnet_build.stdout.log").write_text(result.stdout, encoding="utf-8")
    (work_dir / "dotnet_build.stderr.log").write_text(result.stderr, encoding="utf-8")
    if result.returncode != 0:
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise AssertionError(f"Unity importer compile stub failed with exit code {result.returncode}")

    if args.package_dir is not None:
        package_dir = args.package_dir.resolve()
        if not package_dir.is_dir():
            raise AssertionError(f"Unity importer execution package is missing: {package_dir}")
        execution_report = (
            args.execution_report.resolve()
            if args.execution_report is not None
            else work_dir / "unity_importer_execution_report.json"
        )
        dll_path = work_dir / "bin" / "Debug" / "net8.0" / "MidoriUnityImporterCompileStub.dll"
        run_result = subprocess.run(
            [dotnet, str(dll_path), str(package_dir), str(execution_report)],
            cwd=work_dir,
            text=True,
            capture_output=True,
        )
        (work_dir / "dotnet_run.stdout.log").write_text(run_result.stdout, encoding="utf-8")
        (work_dir / "dotnet_run.stderr.log").write_text(run_result.stderr, encoding="utf-8")
        if run_result.returncode != 0:
            sys.stderr.write(run_result.stdout)
            sys.stderr.write(run_result.stderr)
            raise AssertionError(
                f"Unity importer execution stub failed with exit code {run_result.returncode}"
            )
        print(run_result.stdout.strip())

    print(f"unity importer compile stub ok: {work_dir}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as exc:
        print(f"unity importer compile stub failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
