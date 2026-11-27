# Crystal Ball engine notes

Source evidence is the extracted demo data and static decompilation of the viewer using
`npx -y rea-agents@6.10.1 decompile-all` with Ghidra. Addresses refer to that viewer.
The binary reader consumes each model in full and validates resolved resources and vertex indices.

`castles_setup.txt` names the light castle, dark castle and outside ball models. The DX7 viewer traverses these at `0x401500`, with branches named `LIGHT_INSIDE_BALL`, `DARK_INSIDE_BALL` and `OUTSIDE_BALL`. The files use a group and transform hierarchy, inline render passes, and an environment node at kind `0x100000b1`. Texture paths include their directories, which matters for the repeated brass and window filenames.

The ball is drawn as the original's passes have it: its far wall shows the castle's environment map by the wall's normal, the castle stands inside it, and the glass goes over everything. The glass shows the desert around the ball mirrored, and is more see-through where one looks straight at it; how much comes from the alpha of the environment map in the camera's axes. The dark castle uses object-space normal maps. The original six face lists make the cube maps. The castle change follows the two one-second phases at `0x4048d0`: the glass clouds, the castle and its music change, then the glass clears. The snow uses 2,000 flakes moving on spheres as seen at `0x402350` and `0x404040`, drawn as small quads facing the camera; those that have come down are not drawn.

Approximate or inferred: the old combiners, the optical coefficients, the flight into and out of the ball (1.6 s) and the size of the flakes. Left out: the wallpaper replacement during camera motion, automatic lightning, the UI sprites and the alternate render modes.
