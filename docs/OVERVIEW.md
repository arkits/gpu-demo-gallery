# Overview

## Provenance

The staged ATBRT package lives locally at `atb-runtime/original` and was inspected read-only. `docs/atbrt.txt` identifies it as the Arcade Tool Box (ATB) Runtime Driver Release Notes for Win95/NT, copyright 1997 3Dfx Interactive, Inc. `license.txt` is dated April 1997 and includes 3Dfx license/confidentiality terms.

The staged copy contains installer files, runtime DLLs, `atcmg.lib`, a logo, a license file, and `docs/atbrt.txt`. It does not contain API headers, source files, or format specifications.

## What this repo is for

- Record the package inventory and component roles.
- Map the public `apilib.dll` export surface.
- Document observed ATB-related formats where the decompilation or demo data gives evidence.
- Provide a place for clean-room readers if they are added later.

## What this repo is not

- It is not a copy of the ATBRT SDK.
- It is not a substitute for original 3Dfx documentation.
- It does not claim that decompiled pseudocode is original source.

## Evidence

Primary evidence used by these docs:

- `original/docs/atbrt.txt` and `original/license.txt` for package identity and stated component roles.
- PE export/import tables for `ATB_Demos/Drivers/apilib.dll` and companion DLLs.
- Ghidra 12.2 / REA decompilation at `.rea3dfx/apilib`, whose `decompile-index.json` records target SHA-256 `d6cbc26ebb8543d1e38936b1508e3c0224ba8642d0fc74dff41f1890d0605615`, matching the staged `apilib.dll`.
- Demo-side decompilations in `.rea3dfx/*` for observed BOF usage.

No original binaries were executed.
