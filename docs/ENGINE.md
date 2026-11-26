# Wolfman engine notes

From static analysis of `FurViewerGL.exe` with Ghidra, through `npx -y rea-agents@6.10.1 decompile-all`
and `_rea/dec.sh`, and from the demo's files. The program was not run. Addresses are of that file.
[I] marks inference.

## Drawing

- The second-generation viewer's Scene root names its materials, textures, geometry, transforms,
  animation and render groups. Geometry uses typed vertex streams and 16-bit indices. Every shape is
  drawn with its own passes: `src/vp.rs` and `src/combiner.rs` turn the vertex programs and register
  combiners into WGSL. Matrix palette skinning uses the weighted and indexed vertex streams.
- Fur is registered at `0x412b20`: shell count, spacing and the eight shell textures (DXT3 alpha).
  The program sets the shells' depth to layers × 0.125 × length, length starting at 0.7 and moving by
  0.1 with [ and ] (`0x402390`, `0x402870`); the scene's own 0.5 is not used. Outer shells are drawn
  brighter [I].
- The spotlight `spotLightShape1` casts a shadow map: the fur casts the shadow of the skin below it.
- The street is three tiles, 500 long, that come back at the far end while he walks or runs; how far
  they go is the animation's `groundanim` node.
- The picture is opaque. Nothing reads its alpha (tested), but a page would show through a WebGL canvas.

## Running

- The animation is one clip in ten segments at 30 frames a second, each with its sound (`0x4010c0`):
  scratch, howl, breath to walk, walk (loops), sidestep, walk to run, run (loops), run to walk, walk to
  breath, breathe (loops). A segment's sound repeats if the segment loops, and stops when the next
  begins (`0x4196e0`).
- Until a key is pressed, a script plays again and again (`0x405320`, `0x401760`): breathe twice,
  scratch, howl, walk three times, sidestep, walk, run seven times, and back to breathing.
- Keys 1 to 6 ask for breathe, scratch, howl, walk, sidestep and run (`0x402870`). The first drops the
  rest of the script; each is queued after what is already queued, with the transitions that lead to it
  (`0x401930`). An action ends in a loop, which goes on until the next.
- Music `Wolf 47b LR.wav` repeats (`0x405320`). O is slow motion at 0.3, with the segment's sound
  slowed to match and the music off (`0x402870`). Pausing stops the segment's sound.

## Left out or approximate

- The menus with their pictures, the draw styles, the walking and spherical cameras and the light
  controls. L toggles the logo here; in the original it is the light. M mutes everything, where the
  original's M toggles the music.
- Fins (F). The camera orbits the wolfman from a start chosen to show him, not the original's.
