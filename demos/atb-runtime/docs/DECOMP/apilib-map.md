# apilib decompilation map

## REA/Ghidra metadata

From `.rea3dfx/apilib/decompile-index.json`:

- `version`: 1
- `tool.provider`: Ghidra 12.2
- `target_sha256`: `d6cbc26ebb8543d1e38936b1508e3c0224ba8642d0fc74dff41f1890d0605615`
- `target_path`: `/Users/archit/Dev/gpu-demos/.rea3dfx/work/apilib.dll`
- `layout`: `address`
- `latest_run.selected`: 3447
- `latest_run.excluded`: 0

The target SHA-256 matches the staged `original/ATB_Demos/Drivers/apilib.dll`.

## Function inventory

- 3447 function entries in the index.
- 605 named non-`FUN_*` entries, including 331 exported/public-looking API symbols and many import thunks/externals.
- 2842 `FUN_*` internal functions.

## Key addresses

Addresses below are apilib virtual addresses as decompiled by REA.

| Address | Symbol | Why it matters |
|---|---|---|
| `0x10002b40` | `mgInit` | Initializes message files, site message files, prefs, read-ext flag. |
| `0x10002c50` | `mgNewDb` | Creates new DB through internal helpers. |
| `0x10002ce0` | `mgOpenDb` | File access checks, DB load/create path, current DB setup. |
| `0x10002db0` | `mgCloseDb` | Flush/close/free DB. |
| `0x10003820` | `mgReadImage` | Image read wrapper. |
| `0x100038e0` | `mgReadTexture` | Texture read wrapper. |
| `0x10002eb0` | `mgWriteTexture` | Texture write wrapper. |
| `0x1001b75d` | internal | Image format dispatch by header/magic. |
| `0x1003df20` | internal | `3df` magic check. |
| `0x1003df5d` | internal | `3df` magic check helper. |
| `0x1003dfe0` | internal | Texture attribute sidecar reader; checks magic `0x5444521f`. |
| `0x1004f300` | internal | DB create path used by open/new flows. |
| `0x1005923f` | internal | DB load/parser path used by `mgOpenDb`. |
| `0x1000ee64` | internal | File open/access behavior. |

## How to use this map

- Start from the PE export table for the public API name.
- Jump to the matching `functions/0x*.c` pseudocode for behavior clues.
- Follow internal `FUN_*` calls for format/parser details.
- Record findings as behavior summaries with address citations. Do not paste pseudocode.
