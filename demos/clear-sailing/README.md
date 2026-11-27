# Clear Sailing

A port of NVIDIA's "Clear Sailing" demo for the GeForce FX (2004, OpenGL with Cg) to current
systems: browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with wgpu. A pirate
ship sails at sunset on a choppy ocean that reflects it, with rope rigging, morphing sails,
cannon smoke and an HDR pipeline. It reads the demo's own models, pictures and sounds;
the Cg programs are translated to WGSL by hand in `shaders/demo.wgsl`.

Play it: https://clear-sailing-port.pages.dev

The demo's files are NVIDIA's and are not in this repository. Put the contents of its folder
(`PirateShip.exe`, `models`, `textures`, `audio`, `ui`, `args.txt`, `sliders.txt`) in `data/`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Drag | Turn |
| Up, down, scroll | Zoom |
| f | Fire a broadside |
| h | HDR on or off |
| u | Menus on or off |
| v, -, = | Pick a slider, turn it down or up |
| r | Turning on or off |
| Space | Pause |
| m | Mute |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
