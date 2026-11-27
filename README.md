# GPU demo gallery

[Open the gallery](https://gpu-demo-gallery.pages.dev)

Playable ports of 44 graphics card demos from 3dfx, NVIDIA and ATI, originally released between 1997 and 2006. The old Glide, OpenGL and Direct3D programs now run in the browser with WebGPU or WebGL2. Each demo also has a native player.

These are ports, not recordings or emulators. They read the original models, textures, shaders and audio, then reproduce the rendering, animation and controls in Rust with [wgpu](https://wgpu.rs/). The individual demo folders document the reverse engineering and any remaining differences from the originals.

## Run a demo

The links below open the hosted browser versions. For a local build, pick a demo and follow its README to supply the original assets. Most demos expect them in a git-ignored `data/` directory.

```sh
cd demos/dawn
scripts/dev.sh run   # native player
scripts/dev.sh web   # browser build at a local address
scripts/dev.sh all   # native, WebGPU and WebGL2 checks
```

Each port has its own `Cargo.toml`, development scripts and deployment script. There is no root Rust workspace to build.

## Demos

| Demo | Year | Original GPU | Play | Source |
|---|---:|---|---|---|
| Demoroom | 1997 | 3dfx Voodoo / ATB | [Play](https://demoroom-port.pages.dev) | [Code](demos/demoroom) |
| ATB-TV | 1997 | 3dfx Voodoo / ATB | [Play](https://atb-tv-port.pages.dev) | [Code](demos/atb-tv) |
| Flip | 1997 | 3dfx Voodoo / ATB | [Play](https://flip-port.pages.dev) | [Code](demos/flip) |
| Grand Bleu | 1997 | 3dfx Voodoo / ATB | [Play](https://grand-bleu-port.pages.dev) | [Code](demos/grand-bleu) |
| Racing | 1997 | 3dfx Voodoo / ATB | [Play](https://racing-port.pages.dev) | [Code](demos/racing) |
| Valley of Ra | 1997 | 3dfx Voodoo / ATB | [Play](https://valley-of-ra-port.pages.dev) | [Code](demos/valley-of-ra) |
| Wizards Tower | 1997 | 3dfx Voodoo / ATB | [Play](https://wizards-tower-port.pages.dev) | [Code](demos/wizards-tower) |
| Borg | 1998 | 3dfx Voodoo2 | [Play](https://borg-port.pages.dev) | [Code](demos/borg) |
| Donut | 1998 | 3dfx Voodoo2 | [Play](https://donut-port.pages.dev) | [Code](demos/donut) |
| Surf | 1998 | 3dfx Voodoo / ATB | [Play](https://surf-port.pages.dev) | [Code](demos/surf) |
| 60Hz / 30Hz / 15Hz | 1998 | 3dfx Voodoo / ATB | [Play](https://sixty-hertz-port.pages.dev) | [Code](demos/sixty-hertz) |
| Bubble | 1999 | NVIDIA GeForce 256 | [Play](https://bubble-port.pages.dev) | [Code](demos/bubble) |
| Lightning | 2000 | NVIDIA GeForce 256 | [Play](https://lightning-port.pages.dev) | [Code](demos/lightning) |
| Grove | 2000 | NVIDIA GeForce 256 | [Play](https://grove-port.pages.dev) | [Code](demos/grove) |
| Principles of Shading | 2000 | NVIDIA GeForce2 | [Play](https://principles-of-shading-port.pages.dev) | [Code](demos/principles-of-shading) |
| Creature | 2000 | NVIDIA GeForce2 | [Play](https://creature-port.pages.dev) | [Code](demos/creature) |
| Grass | 2000 | NVIDIA GeForce 256 | [Play](https://grass-port.pages.dev) | [Code](demos/grass) |
| Small Pond | 2000 | NVIDIA GeForce2 | [Play](https://pond-port.pages.dev) | [Code](demos/pond) |
| Toy Soldier | 2000 | NVIDIA GeForce2 | [Play](https://toy-soldier-port.pages.dev) | [Code](demos/toy-soldier) |
| Crystal Ball | 2000 | NVIDIA GeForce 256 | [Play](https://crystal-ball-port.pages.dev) | [Code](demos/crystal-ball) |
| ATI Dolphin | 2001 | ATI Radeon 8500 | [Play](https://radeon-8500-port.pages.dev/dolphin/) | [Code](demos/radeon-8500) |
| ATI Nature | 2001 | ATI Radeon 8500 | [Play](https://radeon-8500-port.pages.dev/nature/) | [Code](demos/radeon-8500) |
| ATI Rachel | 2001 | ATI Radeon 8500 | [Play](https://radeon-8500-port.pages.dev/rachel/) | [Code](demos/radeon-8500) |
| ATI Island 1 | 2001 | ATI Radeon 8500 | [Play](https://radeon-8500-port.pages.dev/island1/) | [Code](demos/radeon-8500) |
| ATI Island 2 | 2001 | ATI Radeon 8500 | [Play](https://radeon-8500-port.pages.dev/island2/) | [Code](demos/radeon-8500) |
| ATI Island 3 | 2001 | ATI Radeon 8500 | [Play](https://radeon-8500-port.pages.dev/island3/) | [Code](demos/radeon-8500) |
| ATI Treasure Chest | 2001 | ATI Radeon 8500 | [Play](https://radeon-8500-port.pages.dev/treasure-chest/) | [Code](demos/radeon-8500) |
| Chameleon | 2001 | NVIDIA GeForce3 | [Play](https://chameleon-port.pages.dev) | [Code](demos/chameleon) |
| Zoltar | 2001 | NVIDIA GeForce3 | [Play](https://zoltar-port.pages.dev) | [Code](demos/zoltar) |
| Bugs | 2002 | NVIDIA GeForce4 Ti | [Play](https://bugs-port.pages.dev) | [Code](demos/bugs) |
| Wolfman | 2002 | NVIDIA GeForce4 Ti | [Play](https://wolfman-port.pages.dev) | [Code](demos/wolfman) |
| Squid | 2002 | NVIDIA GeForce4 Ti | [Play](https://squid-port.pages.dev) | [Code](demos/squid) |
| Tidepool | 2002 | NVIDIA GeForce4 Ti | [Play](https://tidepool-port.pages.dev) | [Code](demos/tidepool) |
| Car Paint | 2002 | ATI Radeon 9700 | [Play](https://car-paint-port.pages.dev) | [Code](demos/car-paint) |
| NPR Hatching | 2002 | ATI Radeon 9700 | [Play](https://npr-hatching-port.pages.dev) | [Code](demos/npr-hatching) |
| Natural Light | 2002 | ATI Radeon 9700 | [Play](https://natural-light-port.pages.dev) | [Code](demos/natural-light) |
| Bear | 2002 | ATI Radeon 9700 | [Play](https://bear-port.pages.dev) | [Code](demos/bear) |
| Dawn | 2002 | NVIDIA GeForce FX | [Play](https://dawn-port.pages.dev) | [Code](demos/dawn) |
| Pipe Dream | 2003 | ATI Radeon 9700 | [Play](https://pipedream-port.pages.dev) | [Code](demos/pipedream) |
| Double Cross v1.4 | 2004 | ATI Radeon X800 | [Play](https://double-cross-port.pages.dev) | [Code](demos/double-cross) |
| Dangerous Curves v1.3 | 2005 | ATI Radeon X850 | [Play](https://dangerous-curves-port.pages.dev) | [Code](demos/dangerous-curves) |
| The Assassin v1.0 | 2005 | ATI Radeon X1800 | [Play](https://the-assassin-port.pages.dev) | [Code](demos/the-assassin) |
| GeoForms | 2006 | NVIDIA GeForce 7 | [Play](https://radeon-8500-port.pages.dev/geoforms/) | [Code](demos/radeon-8500) |
| Box of Smoke | 2006 | NVIDIA GeForce 8 | [Play](https://box-of-smoke-port.pages.dev) | [Code](demos/box-of-smoke) |

## Repository layout

- `web/index.html` contains the gallery's HTML, CSS and JavaScript. It has no frontend framework or package build.
- `demos/<name>/` contains one self-contained port, including its engine notes, tests, original-asset instructions and Cloudflare Pages deployment script.
- `demos/atb-runtime/` contains research and format readers for the 3dfx Arcade Tool Box runtime. It is not a separate gallery demo.
- `scripts/build.sh` assembles `dist/` and renders or crops the gallery screenshots.
- `scripts/deploy-cloudflare.sh` builds and deploys the gallery with Wrangler.
- `docs/PORT_PLAN.md` records the format research and porting plan for the later demos.

## Build the gallery

Install `ffmpeg` for image processing. Rendering missing screenshots also requires the dependencies of the individual ports. Existing screenshots in `dist/img/` are reused when a new render is unavailable.

```sh
scripts/build.sh
```

The generated site is written to `dist/`. Both `dist/` and the screenshot sources are ignored by Git.

To deploy to Cloudflare Pages:

```sh
scripts/deploy-cloudflare.sh              # gpu-demo-gallery
scripts/deploy-cloudflare.sh PROJECT_NAME # another Pages project
```

Wrangler handles the Cloudflare project and deployment. You need to authenticate it with your Cloudflare account first.

## Add a demo

1. Add the port under `demos/<name>/`. New ports belong here rather than in separate repositories.
2. Add a card to `web/index.html` with the demo's name, year, vendor, hardware, description and links.
3. Add a screenshot rule to `scripts/build.sh`.
4. Run `scripts/build.sh` and check the generated gallery in `dist/`.

Keep original demo content and gallery images out of Git.

## Standalone repository history

The ports originally lived in separate `arkits/<name>` repositories. They were merged into this repository on 2026-11-27 with their Git history. Their older commits still use the original repository-root paths, so `git log -- demos/<name>` begins at the import.

For a port imported with a merge commit, inspect its earlier history with:

```sh
git log 'HEAD^{/^Import <name> }^2'
```

A standalone port that still sits beside this repository can be moved in with `scripts/absorb.sh NAME`. The script merges any newer standalone commits, moves the folder into `demos/` and updates the sparse-checkout rules. Commit changes in the standalone repository before running it.

## Original content

The original demos and their art and audio belong to 3dfx, NVIDIA, ATI and Animusic. These ports are unofficial. Original assets are not tracked in this repository; the deployment scripts package them locally for the hosted demos.
