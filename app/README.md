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

`tools-frontend-host-tauri` is not published, and the reviewed checkout it comes
from is not a Git repository, so there is no version or rev to pin it to.
Instead its source is **vendored** into `app/vendor/`, together with
`tools-frontend-contracts` and the three schema files the two crates compile in
with `include_str!` from outside their own directories. The vendored tree is
byte-identical to its source (`diff -r` clean at capture) and contains no Python.

`app/vendor/PROVENANCE.json` records the source path, the capture time, a
SHA-256 for every vendored file, and a tree digest over all of them.
`src-tauri/tests/vendor.rs` recomputes all of it on every test run and also
asserts that `src-tauri/Cargo.toml` still points at the vendored copy and no
longer mentions the shared checkout. Both were mutation-checked: appending a
line to a vendored file and adding an unrecorded file each fail the suite.

That makes the dependency immutable and auditable. It is not the same thing as a
registry version pin, and the README should not be read as claiming it is.

## A note on the generated JSON projection

`midori_contracts::project_from_json` is not a symmetric inverse of
`project_to_json` for any table containing an enum: flatc's serializer writes
`MaterialRole` by name (`"Leaves"`), while the generated parser reads it with
`as_u64().unwrap()`.

This is **pre-existing in the Python generator**, not something the Node port
introduced — the same pairing appears in the checked-in Python-generated
projection for the shared control schema (`OperationStatus` in
`parse_OperationReceipt`, against `impl Serialize for OperationStatus` in
`control_generated.rs`). A byte-identical port is obliged to reproduce it, and
byte-identity against the checked-in bundles is the generator's whole acceptance
basis, so this **must not be "fixed" in the generator**. Changing it is a
contract-tooling decision with that cost attached.

`tests::the_projection_is_not_symmetric_for_enums` pins the behaviour so nothing
here is built on the round trip closing, and so a silent upstream change is
noticed. Nothing on the application's transport path uses the projection.

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

## What is and is not proven

The application **builds and launches**, and the shared shell renders: the menu
bar, command palette affordance, all six docked panels (Source, Preview,
Export & verify, LODs, Result, Output) and a status bar reporting the live
window-host session.

A **user-driven** run of the workflow is *not* proven. Synthetic clicks never
actuated the webview and no CDP handle was obtainable, so nobody has clicked
through open → seed → generate → inspect → export → verify in the running
window. The engine path is covered by Rust tests, the decode and state rules by
vitest under jsdom, and the command/argument names by a static cross-check in
`src/wiring.test.ts` — which says in its own header that it catches the typo
class and nothing more.

If you are screenshotting this application on a scaled display, note that a
DPI-unaware capture reads the physical framebuffer while `GetWindowRect` reports
virtualised coordinates, so it silently returns a magnified crop of the window.
Scale the capture region by `GetDeviceCaps(DESKTOPHORZRES) / GetDeviceCaps(HORZRES)`.
An earlier revision of this file reported a 1.5x dock-layout defect that was
entirely this artefact.

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
