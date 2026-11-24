# Creature

A port of NVIDIA's "Creature" demo (GeForce 3, 2001; Direct3D 7 viewer) to the browser,
macOS and Windows: an animated sea monster under water with caustics, fog, plankton, a
school of 500 fish and a laser, with music.

Rust and wgpu. One code base: WebGPU with a WebGL2 fallback in the browser, Metal on
macOS, Direct3D 12 or Vulkan on Windows.

Play it: https://creature-port.pages.dev

The demo's own files are not in this repository. Unpack the installer (InstallShield) and
put `models`, `animations`, `textures`, `sounds` and `UserInterface` in `data/`, or set
`CREATURE_DATA`.

    scripts/dev.sh run        native player
    scripts/dev.sh web        browser version on port 8080
    scripts/dev.sh all        every check; opens no window
    scripts/dev.sh package    macOS app and Windows exe
    scripts/deploy-cloudflare.sh   public site with the content bundled

Keys: Space fires the laser, + and - add and remove fish, U holds them, P pauses, Home
resets the view, L hides the badge, M mutes, Enter is full screen. Drag to turn, scroll
to zoom.

`docs/ENGINE.md` describes the original and how the port differs.
`docs/DEVELOPMENT.md` covers testing and deployment.

Not verified: running on Windows, Safari, Firefox, phones, Intel Macs, and a side by
side comparison with the original.

The demo's content belongs to NVIDIA.
