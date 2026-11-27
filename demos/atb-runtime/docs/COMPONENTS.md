# Components

Roles below combine the short descriptions in `original/docs/atbrt.txt` with strings and import/export evidence from the staged DLLs. Decompilation-derived behavior is labeled `[I]`.

## Component map

| Component | Stated role | Evidence | Notes |
|---|---|---|---|
| `apilib.dll` | ATB Runtime Driver | 331 named PE exports; imports MGDD/runtime/textureeditor/lm/Win32/CRT; REA index matches SHA-256. | Main ATB API. Exports `mg*` database, record, attribute, geometry, material, light, texture/image, error, prefs, and callback functions, plus `FLC1_*` and `OLD_mg*`. |
| `atcmg.lib` | ATB Runtime Library | Archive contains ATB symbol names such as `mgInit`, `mgOpenDb`, `mgGetTextureTexels`. | Link library for programs using ATB. |
| `fltdata.dll` | ATB Runtime Driver | Strings/exports include `FltDataCodeMap`, `FltRecordCodeMap`, `mgInitAction`, `mgInitDD`, many `flt*` fields. | OpenFlight/FLT data/code map support. |
| `lm.dll` | LM component named by package | `apilib.dll` imports `LM_set_server` and `LM_set_keydir`. | License manager dependency. |
| `mgdd.dll` | ATB Runtime Driver | Strings include `MultiGen Data Dictionary DLL`, `ddGetSchema`, `ddIsFlt`, `ddGetField*`, `ddSite*`; apilib imports many `dd*` symbols. | MultiGen Data Dictionary services used by ATB. |
| `msvcrt40.dll` | Microsoft Visual C++ 4.x Runtime | Stated by `atbrt.txt`; normal CRT redistributable. | Runtime dependency for Win95/NT builds of this era. |
| `runtime.dll` | ATB Runtime Driver | Strings include `Multigen Runtime DLL`, prefs APIs, pool/memory helpers, graphics/sound/file helpers; apilib imports `PIAllocMem0`, `PICreatePool`, `WritePrefsString`, etc. | Platform/runtime services under ATB. |
| `textureeditor.dll` | ATB Runtime Driver | apilib imports `RegisterTextureFile`, `UnregisterTextureFile`, `CopyTextureFile`, `GetTexturePat`. | Texture file registration and pattern/copy support. |

## Dependency sketch

`apilib.dll` sits at the center. `[I]` From imports and decompilation, it calls into MGDD for data dictionary/schema work, runtime.dll for memory/prefs/platform services, textureeditor.dll for texture file operations, and lm.dll for license server/key directory setup. `fltdata.dll` provides FLT-related data/code maps and initialization hooks.

## Install context

`atbrt.txt` says to copy ATB DLLs to the system directory (`\WINDOWS\SYSTEM` or `\WINNT\SYSTEM32`). This repo does not automate installation.
