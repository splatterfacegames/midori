# Midori tree workbench

The first engine-backed vertical slice on the shared tools frontend stack.

The workflow: open a checked-in preset or a local species TOML, edit the seed,
generate in Rust, inspect material-aware geometry and LODs, export a GLB, then
reopen that file and verify its contents.

## Shape

`app/` follows the shared stack's reference workbench: its own Cargo workspace
and lockfile, `tools_frontend_host_tauri::init()` for real native secondary
windows, and a `shared-window-host` capability scoping `tools-frontend-host:default`
to `main` and `host-*`. `src/host.ts` picks `createTauriWindowHost` or
`createBrowserWindowHost` at runtime, so no application code branches on the host.

## The contract is the boundary

Every Tauri command that describes a computation speaks the generated
FlatBuffers contract from `schemas/midori/tree.fbs`. `tree_compute` takes
`TreeComputeRequest` bytes and returns `TreeComputeResult` bytes. There is no
second hand-maintained JSON protocol beside it.

Large payloads stay out of the control envelope. A result carries an
`ArtifactRef` — kind, SHA-256 and byte count — and the GLB bytes travel over
`tree_artifact`, a separate path that re-digests them on the way out and again
on arrival. A result envelope is under 4 KB against a multi-megabyte GLB.

The seed is a `ulong` end to end. TypeScript holds it as a canonical decimal
string and encodes a `bigint`; it never becomes a JS `number`, because 2^64-1
and 2^64 are the same double.

## Dependencies

The npm half of the shared stack is installed from the reviewed packed tarball,
not a checkout alias:

    "@jethac/tools-frontend-stack":
      "file:C:/Users/jetha/AppData/Local/Temp/lab-shared-main-bdade495/package/jethac-tools-frontend-stack-0.1.0.tgz"

`node_modules/@jethac/tools-frontend-stack` is a real directory — not a symlink
or a junction — and the lockfile's `integrity` is the SHA-512 of that exact
tarball, with no `link` field. `npm` rewrites `resolved` to a path relative to
this directory, so the lockfile is not portable to another machine even though
the integrity pin is exact.

`tools-frontend-host-tauri` is still a Cargo **path** dependency on the reviewed
checkout. There is no registry or Git rev for it in the lab and the shared
checkout is not a Git repository, so there is nothing to pin it to. This is the
one remaining dependency gap.

## Known defect: the dock layout does not fit the window

On a display scaled to 150%, every panel is laid out exactly 1.5x too wide and
too tall. `objects`, `inspector` and `bottom` fall outside the window, leaving
only Source, Preview and Export & verify reachable. The factor tracks
`devicePixelRatio` and held at 1.50 across window widths of 1360, 1700, 1900 and
2560 px, which points at the shared `computeGeometry` receiving a container size
in device pixels and applying the result as CSS pixels.

This is not worked around locally. See the comment on `docking.defaultLayout` in
`src/App.tsx`.

## What is deliberately absent

There is no rendered 3D preview. The shared viewport baseline exposes a
`MeshDescriptor` of positions, indices, normals and colour — not GLB loading, UV
or submesh materials, wind hooks or instancing — so a viewport mounted here could
only show a flattened, solid-coloured approximation, which the approved design
says does not pass. Material-aware geometry is inspected instead, and the
exported file is reopened and reported on.

Only source and seed are editable. Structural inspector regeneration, graph-edge
execution, Show Normals and the nature Random control all have source-observed
defects in the legacy frontend; porting them here as if finished would be
shipping known-broken behaviour.

Bark and Leaves are the only material roles, because they are the only ones
public `midori_core::MaterialType` has. Wind is Midori-owned and has no hook
proof yet, so it is not advertised as working.

## Building and running

    npm install
    npm test          # vitest, jsdom
    npm run build     # typecheck + vite build
    npm run desktop   # tauri dev

Rust, from this directory:

    export CARGO_TARGET_DIR='B:/lab/.cargo-target-midori-app'
    cargo test --manifest-path src-tauri/Cargo.toml --jobs 2

`MIDORI_SOURCE_REVISION` is read at build time into the engine identity a result
carries. It is a value the build environment supplied, not an attestation.

No Python anywhere in the generate, build, test or launch path.
