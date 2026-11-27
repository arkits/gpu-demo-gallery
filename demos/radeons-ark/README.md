# Radeon's Ark

A port of ATI's Radeon's Ark (version 1.3, ATI Research's 3D Application Research Group,
2000-2002, Direct3D 7, Radeon) to current systems: browsers (WebGPU, or WebGL2), macOS and
Windows. Written in Rust with wgpu. A four-minute flight, to the demo's music, with Noelle
through an ark of animals: habitats with a gorilla and chimps, a corridor of specimens, and an
aquarium whose tanks of fish, rays, a shark and an orca are seen through bending water with
moving caustics. Floors reflect, chrome reflects its room through dual paraboloid maps, pools
ripple and refract, and every character is skinned with up to four matrices, as the Radeon
blended vertices. Everything is read from the demo's own files.

Play it: https://radeons-ark-port.pages.dev

The demo's files are ATI's and are not in this repository. Put the contents of its
`Radeon'sArk1.3` folder in `data/`.

    scripts/dev.sh run      native player
    scripts/dev.sh web      browser version at http://127.0.0.1:8080/
    scripts/dev.sh all      every check

| Input | Action |
|---|---|
| Space | Pause, play |
| r | From the start |
| m | Mute |
| Enter, Esc | Full screen, quit (native) |

How the original works: [docs/ENGINE.md](docs/ENGINE.md).
