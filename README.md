# ATB Runtime

Notes and format readers for 3dfx's Arcade Tool Box runtime (ATBRT), the 1997 Win95/NT runtime shipped with ATB demos.

This repository is code and documentation only. It does not include 3dfx SDK binaries, demo data, or other third-party content. Keep a local copy of the original ATBRT package outside git if you want to verify checksums or compare against the staged files used for this research.

## What ATBRT is

`docs/atbrt.txt` in the original package identifies it as "Arcade Tool Box (ATB) Runtime Driver Release Notes", copyright 1997 3Dfx Interactive, Inc. The package is a Win95/NT runtime install, not a documented source SDK: the staged copy contains DLLs, an import/static library, installer files, a short release note, and a license file.

## Package contents

The staged original package contains these runtime-facing files under `ATB_Demos/Drivers/`:

| File | Role from `atbrt.txt` | Notes |
|---|---|---|
| `apilib.dll` | ATB Runtime Driver | Main ATB API surface; 331 named PE exports in the staged copy. |
| `atcmg.lib` | ATB Runtime Library | Import/static library with ATB symbols. |
| `fltdata.dll` | ATB Runtime Driver | OpenFlight/FLT data maps and record/field code strings. |
| `lm.dll` | Listed in package; LM.LIB named in text | License manager component used by `apilib.dll`. |
| `mgdd.dll` | ATB Runtime Driver | MultiGen Data Dictionary DLL. |
| `msvcrt40.dll` | Microsoft Visual C++ 4.x Runtime | CRT redistributable. |
| `runtime.dll` | ATB Runtime Driver | MultiGen Runtime DLL; prefs, memory, platform services. |
| `textureeditor.dll` | ATB Runtime Driver | Texture file registration/copy/pattern support. |

See [docs/COMPONENTS.md](docs/COMPONENTS.md) for evidence and dependencies.

## Repository layout

- [docs/OVERVIEW.md](docs/OVERVIEW.md): package scope, provenance, and limits.
- [docs/COMPONENTS.md](docs/COMPONENTS.md): runtime component roles.
- [docs/API/index.md](docs/API/index.md): exported API families from `apilib.dll`.
- [docs/FORMATS/](docs/FORMATS): observed data formats — `bof.md`, `3df.md`, `3ds.md`, `flt.md`, `texture-attr.md`.
- [docs/DECOMP/apilib-map.md](docs/DECOMP/apilib-map.md): decompilation map and key addresses.
- [docs/DECOMP/limitations.md](docs/DECOMP/limitations.md): decompilation limits.
- [scripts/verify-inventory.sh](scripts/verify-inventory.sh): verify a local original stash against the recorded checksums.

## Provenance and limits

- The staged original files were inspected read-only. No original binaries were run.
- API behavior is reconstructed from PE exports and Ghidra/REA pseudocode in `.rea3dfx/apilib`. Pseudocode is not source; behavior notes derived from it are marked `[I]` where useful.
- The original package has no API headers or format specification. Format docs here are marked Observed unless stated otherwise.
- `license.txt` in the original package contains 3Dfx license terms. This repo keeps those terms out of git and avoids committing SDK binaries or extracted assets.
