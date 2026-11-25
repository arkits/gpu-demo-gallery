# GPU demo gallery

One page that links the ports of old graphics card demos:

| Demo | Repository | Site |
|---|---|---|
| Borg (3dfx, 1998) | `borg` | https://borg-port.pages.dev |
| Donut (3dfx, 1998) | `donut` | https://donut-port.pages.dev |
| Bubble (NVIDIA, 1999) | `bubble` | https://bubble-port.pages.dev |
| Lightning (NVIDIA, 2000) | `lightning` | https://lightning-port.pages.dev |
| Grove (NVIDIA, 2000) | `grove` | https://grove-port.pages.dev |
| Principles of Shading (NVIDIA, 2000) | `principles-of-shading` | https://principles-of-shading-port.pages.dev |
| Creature (NVIDIA, 2000) | `creature` | https://creature-port.pages.dev |
| Grass (NVIDIA, 2000) | `grass` | https://grass-port.pages.dev |
| Small Pond (NVIDIA, 2000) | `pond` | https://pond-port.pages.dev |
| Toy Soldier (NVIDIA, 2000) | `toy-soldier` | https://toy-soldier-port.pages.dev |
| Crystal Ball (NVIDIA, 2000) | `crystal-ball` | https://crystal-ball-port.pages.dev |
| Chameleon (NVIDIA, 2001) | `chameleon` | https://chameleon-port.pages.dev |
| Zoltar (NVIDIA, 2001) | `zoltar` | https://zoltar-port.pages.dev |
| Wolfman (NVIDIA, 2002) | `wolfman` | https://wolfman-port.pages.dev |
| Squid (NVIDIA, 2002) | `squid` | https://squid-port.pages.dev |
| Bugs (NVIDIA, 2002) | `bugs` | https://bugs-port.pages.dev |
| Tidepool (NVIDIA, 2002) | `tidepool` | https://tidepool-port.pages.dev |
| Car Paint (ATI, 2002) | `car-paint` | https://car-paint-port.pages.dev |
| NPR Hatching (ATI, 2002) | `npr-hatching` | https://npr-hatching-port.pages.dev |
| Natural Light (ATI, 2002) | `natural-light` | https://natural-light-port.pages.dev |
| Bear (ATI, 2002) | `bear` | https://bear-port.pages.dev |
| Pipe Dream (ATI, 2003) | `pipedream` | https://pipedream-port.pages.dev |
| Box of Smoke (NVIDIA, 2006) | `box-of-smoke` | https://box-of-smoke-port.pages.dev |

`scripts/build.sh` assembles `dist/`, taking pictures from the sibling
repositories: rendered, or left by their tests. `scripts/deploy-cloudflare.sh` publishes it. To add
a demo, add a link to `web/index.html` and a picture to `scripts/build.sh`. Pictures show
the demos' content, so they stay out of Git. Fonts come from Google Fonts.
