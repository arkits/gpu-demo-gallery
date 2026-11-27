# Light Scattering

A port of ATI's "Rendering Outdoor Light Scattering in Real Time" demo (Naty Hoffman of
Westwood Studios and Arcot J. Preetham of ATI Research, GDC 2002, Direct3D 8.1, Radeon 8500)
to current systems: browsers (WebGPU, or WebGL2), macOS and Windows. Written in Rust with
wgpu. Hills under a sky dome, both coloured by sunlight scattered on its way to the eye
(Rayleigh scattering by molecules, Mie by haze), worked out for each vertex. A movie moves the
camera, the sun and the scattering settings from dawn to dusk; the original's sliders change
them by hand. It reads the demo's own height field, pictures and movie.

Play it: https://light-scattering-port.pages.dev

The demo's files are ATI's and are not in this repository. Put the contents of its folder in
`data/`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Sliders (browser) | Sun, its intensity, Rayleigh, Mie, Henyey-Greenstein g, inscattering |
| Up, Down | Forward, back |
| Left, Right | Turn |
| PgUp, PgDn | Pitch |
| Home, End | Up, down |
| F9, F11 | Roll |
| F5, F6 | Smaller, larger steps |
| , . | Sun down, up (not in the original) |
| Drag, scroll | Look, forward and back (not in the original) |
| Space, r | Play or pause the movie, start it again |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
