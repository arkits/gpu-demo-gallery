# Gas Station engine notes

From static analysis of `bin/GasStation.exe` (Last Chance Gas, NVIDIA 2003, NVDemo framework) with
Ghidra 12.2 (full decompile plus the vtable-only methods found through MSVC RTTI), checked against the
disassembly where Ghidra drops control flow (it does: switches with returns, early exits). The program
was not run. Addresses are of that file. [I] marks inference; the rest was read in code or the data.

## Shaders

The viewer compiles every program at start with the shipped Cg runtime (`cg.dll`, Cg 1.1.0003,
4 March 2003): `cgCreateProgram(ctx, CG_SOURCE, text, profile, entry, {"-I<dir>", 0})`, the directory being
the program's path up to its last slash (`0x460130`). Text starting `!!` is loaded as an object.
`tools/cgdll_emu.py` runs that `cg.dll` under Unicorn the same way, so `data/asm/` holds the exact
listings the viewer gave the driver; `nv-asm` translates them to WGSL (fp16/fx12 rounding, condition
codes, relative addressing, forward branches emulated).

Vertex inputs bind by name: a program's varying leaf parameters (`a2v.objCoord`) by their `ATTRn`
resource (`0x41eb40`), matched with geometry arrays by `channelName`.

## Matrices

Storage is a type tag and 16 floats, row-major, column vectors (`0x40a710` takes column-major arguments and
transposes). `0x40ab20` is A·B, specialised by tag. `0x439a50` transposes, `0x42ee50` inverts,
`0x43d190` transforms a point (divides by w with `rcpss`, Intel's ~12-bit reciprocal; the port divides),
`0x439e20` multiplies by the upper 3×3. Matrices go to Cg with `cgGLSetMatrixParameterfr`.

Transforms: world = parent world · local (`0x40ccf0`). A MayaTransform is a chain of matrix nodes set up in
its constructor (`0x436be0`): T · RPT · RP · R · RA · RP⁻¹ · SPT · SP · S · SP⁻¹. R and RA are Euler
rotations by `rotationOrder` (both; Maya would use XYZ for RA); order 0 is Rz·Ry·Rx (`0x42fbb0`). Radians.

## Camera

Fields: `useScreenAspectRatio@0x28 near@0x2c far@0x30 minNear@0x94 dynamicNear@0x98 maxFar@0x9c dynamicFar@0xa0
fieldOfView@0xa4 aspectRatio@0xa8 position@0xac orientation@0xb8 transform@0xcc`.

- View (`0x439ec0`) = inv(world of transform) · R(q)⁻¹ · T(−position); q is (w, x, y, z), conjugated
  (`0x4397f0`) and turned into a matrix (`0x42e6d0`).
- Projection (`0x41ab80`): gluPerspective with `fieldOfView` as the full vertical angle in radians.
- viewProj = proj · view (`0x43a5c0`). Eye position and direction: the last column of view⁻¹ and view⁻¹·(0,0,−1).
- Accessors: +0x100 view, +0x144 projection, +0x188 viewProj, +0xe8 direction, +0xf4 position.

## TransformParam `map` (`0x43f210`)

W window matrix (`0x43ee70`: scale ½, offset ½), V view, P proj, VP = P·V, M object world, B the
transform's +0x38 matrix (joints: inverse bind pose [I]), inv(), T() transpose.

| map | value | map | value |
|---|---|---|---|
| 0 | V | 7 | M |
| 1 | P | 8 | V·M |
| 2 | W | 9 | VP·M |
| 3 | W·P | 10 | W·VP·M |
| 4 | VP | 11 | inv(M) |
| 5 | W·VP | 12–14 | V, P, VP · inv(M) |
| 6 | inv(V) | 15–18 | T(M), V·T(M), P·T(M), VP·T(M) |
| 23 | M·B | 24–26 | V, P, VP · M·B |
| 27 | W·VP·M·B | 28–31 | inv(M·B), and V, P, VP · it |
| 32–35 | T(M·B) and V, P, VP · it | 36–39 | T(inv(M·B)) and V, P, VP · it |
| 40 | (+0x1cc)·M | 41 | P·(+0x1cc)·M |
| 42 | inv(V·M) | | |

## Other parameters

- ConstantParam: `value` with `cgGLSetParameter4fv`.
- TextureParam: binds the texture, then applies its `textureState` (TextureParams) to it.
- MiscParam (`0x43e470`): 0 time ×4; 1 camera position in object space; 2 camera direction in object space
  (normalised with `rcpss`); 3 camera world position; 4 camera world direction (w 1); 5 (near, far,
  far − near, 0); 6 four `rand()` values.
- LightParam (`0x440080`), `space` 0 object, 1 world, 2 eye: property 0 color·intensity (w 0); 1 position;
  2 direction; 3 attenuation (w 1 for point lights, 0 for spots); 4 spot (inner, outer, cos inner,
  cos outer); 5 a half-angle vector. Lights take world values from their transform (`0x441b00`).

## Render passes (`0x42c110`)

Per pass: viewport (`0x4469e0`: from the targets' size, or the window), camera aspect and matrices, every
shader's per-pass parameter update, shapes collected into the pass's render list and sorted, render target
(`0x42b7e0`, `cubeFace`), viewport and clears (`0x42bac0`), the override settings, the backgrounds,
the render list, then the screen pass's UI.

Render list (`0x499220`, `0x499030`, `0x42d240`): a map by `sortSet`; within each set sortType 2 is drawn
first, sorted near to far; then 0 in insertion order; then 1, sorted the same way and drawn in reverse.
The key is the distance of the shape's world bounding centre from the camera along the view
(near + plane distance to the near plane). Frustum culling is computed and ignored. `cull` 3 hides a
shape. World bounds: centre W·c, radius r·|W₃·(1,1,1)/√3| (`0x497970`).

## Render states

Defaults (constructors `0x472e60`…`0x4726a0`): cull ccw-front, cull back; blend ONE/ZERO op 0, all
masks; depth test on, LESS, write; alpha test off, func 1, ref 0; stencil off; fill, line width 1;
texture min/mag/mip filter 2 (linear), wrap 0, bias 0, anisotropy 1. Built-ins (`0x454fb0`…):
`_CullNoCull` cullFace 3; `_BlendAlpha` 4,5,4,5,1; `_BlendAdd` 1,1,1,1,1; `_BlendNoColor` masks off;
`_DepthEqual` EQUAL, no write; `_DepthNoWrite`; `_DepthNoTest` (test and write off); `_AlphaTest0`
on, NOTEQUAL, 0; `_RasterPoint` / `_RasterLine` drawStyle 2 / 1; `_TextureBump` remapped;
`_TextureRepeat` wrap 1; `_TextureShadow` compare, wrap 2.

Enums: cull face 0 FRONT, 1 BACK, 2 FRONT_AND_BACK, 3 off. Blend factor 0 ZERO, 1 ONE, 2 SRC_COLOR,
3 1−SRC_COLOR, 4 SRC_ALPHA, 5 1−SRC_ALPHA, 6 DST_COLOR, 7 1−DST_COLOR, 8 DST_ALPHA, 9 1−DST_ALPHA,
10 SRC_ALPHA_SATURATE, 11–14 constant color/alpha. Blend op 1 ADD, 2 SUBTRACT, 3 REVERSE_SUBTRACT,
4 MIN, 5 MAX. Compare 0 NEVER, 1 LESS, 2 LEQUAL, 3 GREATER, 4 GEQUAL, 5 EQUAL, 6 NOTEQUAL, 7 ALWAYS.
Stencil op 0 KEEP, 1 ZERO, 2 REPLACE, 3 INCR, 4 DECR, 5 INVERT, 6 INCR_WRAP, 7 DECR_WRAP. Wrap 0
CLAMP_TO_EDGE, 1 REPEAT, 2 CLAMP_TO_BORDER, 3 MIRRORED_REPEAT (none on rectangles). Filters 1 NEAREST,
2 LINEAR; mip filter 0 none, 1 nearest, 2 linear (`0x415560`); GL_GENERATE_MIPMAP is on. Alpha reference
is 0–255. Texture formats (`0x4152d0`): 0 L8, 1 L8A8, 2 RGB5, 3 RGB8, 4 RGBA8, 5 SIGNED_RGB8, 6 SIGNED_RGBA8,
7 SIGNED_HILO8, 8 HILO8, 9 SIGNED_HILO16, 10 HILO16, 11–13 DXT1/3/5, 14 DEPTH16, 15 DEPTH24, 16–19 FLOAT
R/RG/RGB/RGBA16, 20–23 the same at 32 bits; others (24) the file's own.

## Animation

Channels set a field of a target node by name. A Maya curve (`0x475200`, `0x47b2d0`, `0x47b1b0`): keys of
8 words (time, value, two tangent kinds, in x, out x, in y, out y). Hermite with slopes out y / out x and
in y / in x (5729578 where x is 0). Outside [start, stop] (`0x4745c0`): modes 0–1 hold the end value,
2 cycles, 3 cycles and adds (last − first key value) per cycle (the count is n + 1, positive on both
sides), 4 oscillates (as the code has it, which is not mirror-symmetric).

## Geometry

Strips (5) restart at 0xffff when `useRestartPrimIndex`; lists (3).

## Gas Station's interaction

Input goes through GLUT (statically linked): keyboard `0x4079e0`, special keys `0x401330`, mouse
`0x4012c0`, motion with a button `0x404e80`, motion without `0x4012f0` (the window class's slots
+0xc…+0x1c call glutKeyboardFunc, glutSpecialFunc, glutMouseFunc, glutMotionFunc — event mask
0x700 — and glutPassiveMotionFunc). Only a drag (motion with a button) takes the walking camera
and resets the idle clock; keys never do.

The viewer's mouse state (`viewer + 0xbc`): +0xc the last event went to a menu, +0x14/15/16 left,
middle, right held, +0x18 where the left press was, +0x30 the previous position, +0x38 the
current one, as fractions of the window. A left press goes to the menus first (`0x467780`).

Walking (`0x402e30`, the collision version of NVWalkNavigator's update): nothing moves if the
last mouse event went to a menu. The arrows ease (`0x42e040`: clamp(5 Δt, 0, 1), or one minus that
after a release) into x = (right − left)/2, z = (down − up)/2; a left drag adds the drag's height
to z and turns yaw by −Δx·dt·π. The step, times speed 20 and dt, turns by yaw and is checked
against `collision1.tga` (`0x402bf0`, raw file order: blue, green, red, bottom row first): open
ground is (255, 127, 127), black stops, other pixels are wall normals (red, green)·2/255 − 1 off
which the step slides if the slid point is open. The map's edge clamps to the extents
(−32.416…22.913, 27.647…−16.906). A right drag looks (`0x42e2c0`), pitch within ±1.5697963.

Buttons (`ButtonCB 0x406740`) read "on" as available: the mode in use is off. Wireframe is a pass
override (`0x457f60`): no culling, no depth test, lines, `_white`. The slider menu starts hidden;
the clouds and shadow sliders start at their values, the speed slider at its own 0 (`0x40965f`).
