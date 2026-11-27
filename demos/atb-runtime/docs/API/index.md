# apilib API

This is an inventory of the staged `apilib.dll` public export surface. It is not an original header. Names and ordinals come from the PE export table; behavior notes come from selected Ghidra/REA decompilation and are marked `[I]` when inferred.

## Export summary

- 331 named exports, ordinals 1-331.
- Mostly `mg*` functions.
- 7 `FLC1_*` color conversion exports.
- 3 `OLD_mg*` compatibility exports.

## Families

### Database and file lifecycle

`mgInit`, `mgExit`, `mgNewDb`, `mgOpenDb`, `mgCloseDb`, `mgSaveAsDb`, `mgWriteDb`, `mgSetCurrentDb`, `mgGetCurrentDb`, `mgRec2Db`, `mgRec2Filename`.

- `[I]` `mgInit` (`0x10002b40`) initializes message files, all site message files, prefs, and read-extension flag handling.
- `[I]` `mgOpenDb` (`0x10002ce0`) checks file access, may create/open through internal helpers, parses/loads DB data, allocates a DB wrapper, and sets current DB.
- `[I]` `mgNewDb` (`0x10002c50`) creates a new DB through internal helpers and wraps it as the current DB.
- `[I]` `mgCloseDb` (`0x10002db0`) flushes/closes internal DB state and frees handles.

### Records and graph traversal

Representative exports: `mgNewRec`, `mgDelete`, `mgAttach`, `mgDetach`, `mgAppend`, `mgInsert`, `mgReference`, `mgDeReference`, `mgDuplicate`, `mgGetChild`, `mgGetParent`, `mgGetNext`, `mgGetPrevious`, `mgCountChild`, `mgGetChildNth`, `mgGetRecByName`, `mgGetName`, `mgSetName`, `mgGetNestedParent`, `mgGetNestedNext`, `mgGetNestedPrevious`, `mgGetNestedFirst`, `mgGetReference`, `mgGetFirstInstance`, `mgGetNextInstance`.

Node-type predicates include `mgIsBsp`, `mgIsDof`, `mgIsGroup`, `mgIsHeader`, `mgIsLightSource`, `mgIsLod`, `mgIsObject`, `mgIsPolygon`, `mgIsSound`, `mgIsSwitch`, `mgIsPath`, `mgIsVertex`, `mgIsXref`, and related `mgGet*` accessors.

### Attributes, values, and types

Representative exports: `mgGetAttList`, `mgGetFirstAtt`/`mgGetNextAtt`, `mgGetAttCode`, `mgGetAttType`, `mgHasAtt`, `mgGet/SetAttBuf`, `mgGet/SetAttList`, `mgGet/SetAttRec`, `mgGet/SetChar`, `mgGet/SetText`, `mgGet/SetDouble/Float/Int/Short/UInt/UShort`, `mgGet/SetVal*`, `mgGetCode`, `mgGetType`, `mgIsCode`, `mgIsType`, `mgIsFlagSet`, `mgSetFlag`.

Compatibility exports: `OLD_mgGetFlag`, `OLD_mgGetAttFlag`, `OLD_mgFindExtendRec`.

### Geometry and transforms

Representative exports: `mgCoord3f/2f/3d/2d`, `mgIcoord3*`, `mgFCoord3*`, `mgVector*`, `mgMakeLine`, `mgMakeVector`, `mgVectorFromLine`, `mgUnitizeVector`, `mgCrossProdVector`, `mgDistance`, `mgGetPolyNormal`, `mgGetVtxNormal`, `mgIsPolyConcave`, `mgTriangulate`, `mgSplitBigPoly`, `mgReverse`, `mgIsCoplanar`, `mgTransformCoord`, `mgMatrixFormRotate`, matrix get/set helpers.

### Materials, lights, colors, palette

Representative exports: `mgNewMaterial`, `mgNewMaterialName`, `mgGetMaterial*`, `mgDeleteMaterial*`, `mgSetCurrentMaterial*`, `mgGetMaterialCount`, `mgNewLightSource`, `mgGetLightSource*`, `mgIndex2RGB`, `mgRGB2Index`, `mgReadColorPalette`, `mgWriteColorPalette`, `mgGetCurrentColor*`, `mgSetCurrentColor*`, `FLC1_RGB2I`, `FLC1_I2RGB`, `FLC1_CMY2I`, `FLC1_I2CMY`, `FLC1_HSV2I`, `FLC1_I2HSV`, `FLC1_Init`.

### Textures and images

Representative exports: `mgReadImageInfo`, `mgReadImage`, `mgWriteImage`, `mgReadTexture`, `mgReadTextureInfo`, `mgReadTexturePalette`, `mgWriteTexture`, `mgGetTextureCount`, `mgGetTextureIndex`, `mgGetTextureName`, `mgSetTextureName`, `mgGetTextureWidth/Height`, `mgGetTextureTexels`, `mgGetTexturePosition*`, `mgSetTexturePosition*`, `mgGetTextureGeoNum`, `mgInsertTexture`, `mgReplaceTexture`, `mgCopyTexture`, `mgDeleteTexture`.

- `[I]` `mgReadImage` (`0x10003820`) wraps an internal image reader.
- `[I]` `mgReadTexture` (`0x100038e0`) resolves DB/texture index and loads texture data through internal helpers.
- `[I]` `mgWriteTexture` (`0x10002eb0`) resolves texture attributes/pixels and writes through an internal writer.
- `[I]` Internal image dispatch (`0x1001b75d`) checks file headers/magic and selects format-specific readers. A `3df` magic check appears in `0x1003df20`/`0x1003df5d`.

### Errors, messages, prefs, callbacks, memory

Representative exports: `mgSendMessage`, `mgSendError`, `mgSendWarning`, `mgSendStatus`, `mgSetMessagesEnabled`, `mgSetWarningEnabled`, `mgSetErrorEnabled`, `mgSetStatusEnabled`, `mgGetMessageText`, `mgInitPrefs`, `mgSetReadExtFlag`, `mgSetOpenCreateFlag`, `mgSetNewOverwriteFlag`, `mgRegisterCallback`, `mgRegisterPreActionCallback`, `mgRegisterValueActionCallback`, `mgRegisterPostActionCallback`, `mgCall`, `mgWalk`, `mgMalloc`, `mgFree`.

## Caveats

- Argument names/types are not authoritative without headers.
- Use the PE export table as the symbol inventory and decompilation only for behavior hints.
- Do not copy decompiled pseudocode into this repo; cite addresses instead.
