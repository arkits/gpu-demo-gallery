# How S3D Victory works

`DEMO.EXE` is a DOS/4GW (LE) program built with Watcom. REA does not open LE files, so
`_rea/new4/victory/le.py` lays its two objects out at their bases (code at 0x10000, data at
0x30000) with the 3424 fixups applied; Ghidra reads that image as raw 32-bit x86. Addresses
below are in it. Nothing was run. [I] marks inferences.

## Scripts (0x13dfc)

`SCRIPT.SCP` (the default) and `SCRIPT2.SCP` name bitmaps and textures, list groups
(`RELATIONS`), and give each object (`BEGIN file.voj` … `END`) a program. Words are split at
blanks, commas and parentheses. Numbers with a point are stored in hundredths
(`4.00` → 400), others as written. Commands (table at 0x30edc) and their words (0x30b44)
become numbers; the interpreter is 0x103d0, run once a frame for every object.

Each frame an object runs commands until one makes it wait, then moves:

- `TURN`: adds to turning speeds, in hundredths of a degree a frame. Speeds about the object
  itself (`RELATIV`, the default) and about its place (`ABSOLUT`, which makes letters circle
  their group's middle) are kept apart. `ONCE` turns at once instead, `STOP` stops,
  `RESET` straightens. `FRAMES n` waits n frames.
- `MOVE`: sets (not adds) speeds; x and y in pixels, z in hundredths. `FRAMES` waits.
- `SET`: `POSITION` (pixels, z in hundredths), `RELATION` (offset from the group's middle;
  `MASTER g` makes this object's turned middle the place of group g, `SLAVE g` follows it),
  `OBJECT ON/OFF`, `BITMAP NAME b ON`, and `TRI_LIST n` with `TEXTURE`, `RENDER`
  (`UNLIT_TEXTURE`, `UNLIT_PERSPECTIVE`, `GOURAUD`, …), `TEX_UV` (`XY`, `XZ`, `YZ`),
  `TEX_STRETCH` and `FILTER` (`NEAREST`, `LINEAR`).
- `SIZE`: scales the vertices for good, by a tenth of the number (100 if left out); the box
  only grows.
- `WAIT n`, `LOOP n` … `END`. `LOOP n` runs n + 1 times, `-1` for ever; the whole program is
  an endless loop, so it starts again at its end.
- Unused here: `FOG`, `DANZING_ON/OFF`, `TRANS_MORE/LESS`, `WAIT_TIC`, `DELAY`, sync and
  the clock and counter modes.

## Objects (`.VOJ`, 0x12590)

`COLORLIST` (named colors), `VERTEXLIST` (named points), then `STRIPLIST`, `FANLIST` and
`TRILIST`, each with `COLORS` (alpha, then a named color or red green blue) and `LIST`.
A list starts as `UNLIT_TEXTURE`, nearest, with an empty texture record (0x31298).

## Placing (0x11546)

Angles wrap at 36000 and become radians times 0.0001746532 (0x300ed), a little more than
π / 18000, so a turn is 6.2875 radians. A vertex is turned about the object, the offset
added, scaled by 1 / (z 0.005) (0x117bc), turned about the place, and projected with
`x = place.x − X 480 / z`, `y = place.y − Y 480 / z` (the screen block at 0x11d60: 640 by
480, scale 480). Depth is z times 100 [I: for the ViRGE's depth buffer, which is allocated
but whose state is not set in the frame loop; the port tests depth].

## Texture coordinates (0x10040)

From the object's box on the list's plane, in texels: `u = (max − x stretch) (w − 1) /
(max − min)`, likewise v. When the stretch in u is one, z is not stretched in v, as the
program has it. Lists that no script sets keep zero coordinates and texture record zero,
which points at the start of video memory, the screen: they show the screen's first pixel
[I], which the port takes from the background picture.

## Drawing (0x11dbc)

The background bitmap if one is on, else black; then for every shown object every list,
with the S3D Toolkit's render type, filter (1TPP or 4TPP) and texture. `UNLIT_TEXTURE` is
affine (straight across the screen), `UNLIT_PERSPECTIVE` perspective-correct. A frame of the
original is taken as a 60th of a second [I].

## Differences from the original

- The 640 by 480 screen is kept whole and in shape inside the window.
- Four samples a pixel; colors at full depth instead of the ViRGE's 16 bits.
- The frame rate display (`Y`) is left out; `1`, `2` and Space are new.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
