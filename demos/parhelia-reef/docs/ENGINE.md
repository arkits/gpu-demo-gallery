# How the Reef demo works

`FishDemoClient.exe` runs Matrox's TDRG engine on Direct3D 8. Addresses below are in it.

## Scripts

`Data/Scenes/ReefDemoScene.w` (the world) and `.s` (the shaders) are text with each byte
stored plus a byte of a repeating key; the keys are two of the program's own log messages
(`*** TDRG_Audio::Mix() - Software Mix ENABLED` and
`*** TDRG_Input::QueryDevice() - m_poInputDevice FAILED`). Decoded they are trees of
`Name { value }`.

The world holds 150 mesh actors, 2 logo actors, 11 cameras, a light and 36 relationships.
An actor names an .X file, a place, a turn in degrees (S·Rz·Ry·Rx·T, 0x46f9c0) and a shader
for each material: `model.frame.Mesh_<n>.<material>` (or `Shader_<n>` for an unnamed one),
where n counts meshes through the whole file. Names start `[Sort:NN]` and are drawn in that
order; logo actors are screen overlays drawn without depth, after everything.

Relationships put cameras (and the silver fish) on bones of animated helper models: the tour
camera sits on `Demo_CAM` of `Anim_Camera_Position.x` and looks at `Demo_CAM_Target`. Bones
are taken in the model's own space; the helpers' places in the world file are unused.
`SwitchCamera` (0x4013b0) steps through the cameras.

## Shaders

A shader is passes; a pass is render states, a vertex and a pixel shader, up to four texture
layers, and constants. Blend factors and compare functions are D3D's numbers less one; cull
mode is 0 none, 1 counter-clockwise (D3D's usual), 2 clockwise. Textures are TGA (rows from
the bottom unless the header says otherwise) with each mip level a file, `NAME_M00.tga` on;
the caustics are a volume of 32 slices, `NAME_L00_M00.tga` on.

The vertex layout comes from the pass's FVF (0x480f40): position v0, blend weights v1 and
indices v2, normal v3, colours v5 and v6, texture sets from v7, tangent v14, binormal v15. The
reef uses four texture sets, of which the third and fourth (one number each) are the blend
between rock and sand (v9) and a light attenuation (v10). Empty sets in the .X file are
skipped. Skinned meshes are split into batches of at most 12 bones.

Constants (SetConstants, 0x499cd0), `Constant { register component type ... }`:

| Type | Value |
|---|---|
| 1 | a number |
| 2 | a variable (0x49a160): 0 seconds, 1 frames, 3 back buffer height, 4 width |
| 3 | a wave (0x49a290): f(x·k + p)·A + offset, f sin, cos, A/x, exp, ln, abs, fmod, linear, square |
| 5 | a position: the camera or the object |
| 6 | a matrix, transposed: world, view, projection or products of them, a frame, identity |
| 7 | a palette of bone matrices |
| 12 | a "data bunny": a number animated in `Anim_Fade.x`, for the fade in and out |

The world shader's constants (light, camera, fog, a gentle swaying of the picture, the
caustics' scroll) are set first each frame and stay in the registers for every draw.

## Animation

Animation keys are in each model's .X file, in milliseconds. Rotation keys are the four
numbers of a D3DXQUATERNION in memory order (x, y, z, w); read so, they begin at each frame's
own pose. The tour lasts 95 seconds; each actor may start its animation later
(`AnimationOffset`).

## Not ported

Passes may ask for their draws (`SortMode`) or their triangles (`PGSortMode`) to be sorted
by depth: the translucent fins of eight fish, the jellyfish, two translucent corals and the
sun glare. The port draws them in scene order, from triangles kept in the file's order, so
where a translucent part overlaps itself the blend can differ. The music is WMA, played here
from FLAC made from it.
