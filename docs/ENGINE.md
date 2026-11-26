# Bugs engine notes

The port uses the original scene data and static analysis of the x86 Windows OpenGL
viewer, `BugsGL.exe` (SHA-256 `6eb8c3c0e73966c072ecda587d5eb43d297ae7d4c58c7ce3d171b10bbb863267`).
Function addresses below are from REA 6.10.1 with Ghidra 12.2.

| System | Viewer addresses and behavior |
| --- | --- |
| Terrain | `0x481a90`, `0x4a12d0`, `0x4a4640`. Each 100-cell axis has 101 vertices. The height sampler blends four intensities in UV space and maps each byte to `800 × intensity / 256 − 400`. The TGA loader reverses the file rows and the sampler reads the first byte of BGR pixels. |
| Lighting and shadows | `0x408c20`, `0x408d70`, `0x40acc0`, `0x40cd70`. The moving spotlight drives a 512² projected shadow map with a 0.5-radian field of view. Nearby junk and the shadow-only `bug0` mesh are drawn in 0.2 grey. The visible bugs use a directional bump light and the constant `(1, 1, 1, 0.3)`. |
| Flock and show | `0x407ec0`, `0x4142f0`, `0x415100`, `0x40c100`, `0x40cb10`, `0x40d3c0`. The show opens on the empty stage, follows one bug through the opening phases, then adds the horde over 23 seconds. The swarm crosses the stage from x = −2000 and resets beyond x = 500. Bugs steer around seven junk locations, align with nearby bugs, tire, stop, and recover. |
| Legs | `0x4018c0`, `0x403230`, `0x404dd0`. Two groups of three feet take turns stepping. The foot target stays fixed in world space until it moves outside the standing radius. The leg solver works back from this target. Body height averages 21 units and varies on two frequencies. |
| Vision, effects and inset | `0x409730`, `0x40ad70`, `0x40ae30`, `0x40b610`, `0x40a9c0`, `0x40d3c0`. Vision overrides suppress the sky. Depth Vision clears to `(0.1, 0.2, 0.3)` and accumulates draw passes with depth testing disabled. Normal mode draws the fan, dust and flare. The single-sample inset draws a crop of the main view. |
| Sound | `0x40a340`, `0x40c100`, `0x40cb10`, `0x40d3c0`. The helicopter emitter follows the spotlight position. The horde rumble ramps up over 15 seconds. The six positional bug clips are selected when a bug rests. |

The renderer translates the scene's fixed-function and register-combiner material passes
to WGSL. The 180- and 400-unit visible LOD thresholds come from the original demo.
The same simulation and renderer run natively, on WebGPU and on WebGL2.

The port advances on a fixed 60 Hz clock. Flock trajectories, foot arcs and sound
attenuation approximate the executable's behavior. In Night Vision, the draw callback at
`0x408820` sets grayscale shader constants for the stage and switches to red and
green for the bug groups. The fragment color is weighted by texture alpha. The browser controls
are HTML. The native menu uses the original button artwork. These behaviors were
checked against the disassembly and tested in the port. The Windows executable has
yet to be run for a frame-by-frame comparison.
