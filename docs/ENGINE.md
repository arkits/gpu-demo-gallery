# Crystal Ball engine notes

Source evidence is the extracted demo data and static decompilation of the viewer using
`npx -y rea-agents@6.10.1 decompile-all` with Ghidra. Addresses refer to that viewer.
The binary reader consumes each model in full and validates resolved resources and vertex indices.

`castles_setup.txt` names the light castle, dark castle and outside ball models. The DX7 viewer traverses these at `0x401500`, with branches named `LIGHT_INSIDE_BALL`, `DARK_INSIDE_BALL` and `OUTSIDE_BALL`. The files use a group and transform hierarchy, inline render passes, and an environment node at kind `0x100000b1`. Texture paths include their directories, which matters for the repeated brass and window filenames.

The outer globe reflects the desert. Its inner shell samples the selected castle’s environment through refraction. The dark castle uses object-space normal maps. The port implements those effects with WGSL and uses the original six face lists. The transition and soundtrack changes follow the two one-second phases at `0x4048d0`. The snow uses 2,000 flakes moving on spheres as seen at `0x402350` and `0x404040`.

The port approximates the old combiners, optical coefficients, camera choreography, and snowflake geometry. It loads the entire selected castle when entering the ball. The original wallpaper replacement during camera motion, automatic lightning, UI sprites and alternate render modes are not reproduced.
