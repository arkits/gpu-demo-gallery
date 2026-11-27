# Dawn engine notes

From static analysis of `bin/Fairy.exe` (built 2003-02-11) with Ghidra through
`npx -y rea-agents@6.10.1 decompile-all`, and from the demo's files. The program was not run.
Addresses are of that file. [I] marks inference; the rest was read in code or checked against the data.

## Files

`Dawn.exe` is a self-extracting cabinet around an InstallShield 6 one. The four `.nmb` scenes
use the tree framing of the earlier viewers, with new kinds: kind, numbered fields, -3 name, -4 end.
`src/tree.rs` reads `fairy.nmb`, `fairy_ultra.nmb`, `branch.nmb` and `sunning.nmb` to the last byte.

- Vertex arrays name the shader input they feed (`a2v.coord`, `a2v.coordMorph17`). Many are half floats.
- Key frames are 32 bytes: time, value, two tangent kinds, then in x, out x, in y, out y.
- Curve time is seconds. `timeScale` is 0.5, so the 23.3 s of curves last 46.7 s.
- Strips (5) restart at 0xffff. Bone index -1 is unused.
- `remapped` textures hold -1 to 1. Wrap 0 clamps and 1 repeats (`0x420460`).
- DDS and TGA rows start at the bottom and are given to OpenGL as they are.
- `dappling.tga`, which the branch names, was never shipped. No fragment program reads it.

The port draws the ultra data (`args_ultra.txt`). The other set has simpler shaders and meshes.

## Drawing

`vp30` programs ship as Cg source and `fp30` ones as assembly or Cg. `shaders/dawn.wgsl` has one vertex
program for the 36 and a function for each of the fragment programs that the scenes use.

- A matrix property of 7 carries a mesh; 23 bends it: the joint's matrix times the inverse of where it
  rests in the file. At rest these are the identity (tested).
- Joint matrix: T · jointOrientation · rotation · rotationOrientation · S (`0x43b480`).
- Curves: Hermite (`0x468d60`), as in the earlier viewers.
- Morphing: the first five targets whose weight is over 0.01 (`0x408ed0`). The port adds them up on the CPU.
- Order: by set, then near to far (2), then far to near (1). [I] for the order of those two within a set.
- `_BlendAdd` is one, one [I].
- Cloth is drawn closer: slope -2, and `max(50000 - 2500 d, 100)` units for one kind and half for the other,
  where d is the distance to the root joint (end of `0x40b1b0`).
- Cube maps are crosses. `0x460a60` cuts them: from the bottom row up, +Y turned half way; then +X, -Z, -X;
  then -Y and +Z turned. The normalization cube confirms the row order. The background turns y over in its
  texture coordinates (`0x451e20`). The highlight map is authored upside down to match that, but the diffuse
  and specular maps are not, so in the original too the light on the fairy is that of the forest upside down.
- The background's lens is `0.02 |eye| + 1.5` times 36°.
- Hair (`0x49b310`): 563 guide curves, cubic Bezier, in 903 triangles. Each triangle gets
  `area · 1200` hairs at random places from an unseeded `rand`: 7,499 hairs of five segments, one pixel wide,
  with alpha to coverage. The port keeps their width a 768th of the picture's height.

## Running

- Dance: 43 s. Sunning: 70 s, with the branches elsewhere. Each starts white for one second, fades in for
  three, and fades to white in its last three (`0x40b1b0`, lists made in `0x411530`). Sound follows the fade,
  but is 0.1 where that is zero.
- Dance camera: `FairyCam`, 36° of height. The port uses the window's shape, not the camera's 1.5.
- Free camera: looks at the root joint, up to 6 higher when closer than 40. Its lens goes from 15° at a
  distance of 22 to 54° at 60. Pitch -90° to 45°, at most 50 away (`0x42fea0`).
- Expressions: eight sets of five targets and weights (`0x40c0c0`), and slider labels (`0x5483b0`).
- Oil is ten times its slider, goose bumps six times less three (`0x405520`, `0x4055d0`). They start at 1.
- After 120 s without input the dance returns.

## Left out or approximate

- The 12-bit fixed-point registers are clamped but not rounded.
- Joints that the sunning does not move rest; in the original they stay as the dance left them.
- Near and far are 1 and 200; the movie camera's near plane depends on the scene in the original.
- The wireframe and point menus, the original menu's pictures, and a hidden typed word that shows sliders.
