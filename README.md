# Principles of Shading

A port of NVIDIA's "Principles of Shading" demo (2000, OpenGL, register combiners) to
the browser, macOS and Windows. It draws the NVIDIA logo step by step (vertices,
wireframe, lighting, texture, reflection, everything) in three materials, with music.

Rust and wgpu. One code base: WebGPU with a WebGL2 fallback in the browser, Metal on
macOS, Direct3D 12 or Vulkan on Windows.

Play it: https://principles-of-shading-port.pages.dev

The demo's own files are not in this repository. Unpack `PrinciplesOfShading.exe` (a
self-extracting archive holding an InstallShield cabinet) and put `models`, `Textures`,
`UserInterface` and `sounds` in `data/`, or set `POS_DATA`.

    scripts/dev.sh run        native player
    scripts/dev.sh web        browser version on port 8080
    scripts/dev.sh all        every check; opens no window
    scripts/dev.sh package    macOS app and Windows exe
    scripts/deploy-cloudflare.sh   public site with the content bundled

Keys: left and right step, up and down or 1 to 3 change the logo, Home and End go to the
first and last step, Space pauses, M mutes, L hides the badge, Enter is full screen.
Drag to turn, scroll to zoom.

`docs/ENGINE.md` describes the original and how the port differs.
`docs/DEVELOPMENT.md` covers testing and deployment.

Not verified: running on Windows, Safari, Firefox, phones, Intel Macs, and a side by
side comparison with the original.

The demo's content belongs to NVIDIA.
