# GPU demo gallery

One page that links the ports of old graphics card demos:

| Demo | Repository | Site |
|---|---|---|
| Borg (3dfx, 1998) | [borg](https://github.com/arkits/borg) | https://borg-port.pages.dev |
| Donut (3dfx, 1998) | [donut](https://github.com/arkits/donut) | https://donut-port.pages.dev |
| Bubble (NVIDIA, 1999) | [bubble](https://github.com/arkits/bubble) | https://bubble-port.pages.dev |
| Lightning (NVIDIA, 2000) | [lightning](https://github.com/arkits/lightning) | https://lightning-port.pages.dev |
| Grove (NVIDIA, 2000) | [grove](https://github.com/arkits/grove) | https://grove-port.pages.dev |
| Principles of Shading (NVIDIA, 2000) | [principles-of-shading](https://github.com/arkits/principles-of-shading) | https://principles-of-shading-port.pages.dev |
| Creature (NVIDIA, 2000) | [creature](https://github.com/arkits/creature) | https://creature-port.pages.dev |
| Grass (NVIDIA, 2000) | [grass](https://github.com/arkits/grass) | https://grass-port.pages.dev |
| Small Pond (NVIDIA, 2000) | [pond](https://github.com/arkits/pond) | https://pond-port.pages.dev |
| Toy Soldier (NVIDIA, 2000) | [toy-soldier](https://github.com/arkits/toy-soldier) | https://toy-soldier-port.pages.dev |
| Crystal Ball (NVIDIA, 2000) | [crystal-ball](https://github.com/arkits/crystal-ball) | https://crystal-ball-port.pages.dev |
| Chameleon (NVIDIA, 2001) | [chameleon](https://github.com/arkits/chameleon) | https://chameleon-port.pages.dev |
| Zoltar (NVIDIA, 2001) | [zoltar](https://github.com/arkits/zoltar) | https://zoltar-port.pages.dev |
| Wolfman (NVIDIA, 2002) | [wolfman](https://github.com/arkits/wolfman) | https://wolfman-port.pages.dev |
| Squid (NVIDIA, 2002) | [squid](https://github.com/arkits/squid) | https://squid-port.pages.dev |
| Bugs (NVIDIA, 2002) | [bugs](https://github.com/arkits/bugs) | https://bugs-port.pages.dev |
| Tidepool (NVIDIA, 2002) | [tidepool](https://github.com/arkits/tidepool) | https://tidepool-port.pages.dev |
| Car Paint (ATI, 2002) | [car-paint](https://github.com/arkits/car-paint) | https://car-paint-port.pages.dev |
| NPR Hatching (ATI, 2002) | [npr-hatching](https://github.com/arkits/npr-hatching) | https://npr-hatching-port.pages.dev |
| Natural Light (ATI, 2002) | [natural-light](https://github.com/arkits/natural-light) | https://natural-light-port.pages.dev |
| Bear (ATI, 2002) | [bear](https://github.com/arkits/bear) | https://bear-port.pages.dev |
| Pipe Dream (ATI, 2003) | [pipedream](https://github.com/arkits/pipedream) | https://pipedream-port.pages.dev |
| Double Cross v1.4 (ATI, 2004) | [double-cross](https://github.com/arkits/double-cross) | https://double-cross-port.pages.dev |
| Dangerous Curves v1.3 (ATI, 2005) | [dangerous-curves](https://github.com/arkits/dangerous-curves) | https://dangerous-curves-port.pages.dev |
| The Assassin v1.0 (ATI, 2005) | [the-assassin](https://github.com/arkits/the-assassin) | https://the-assassin-port.pages.dev |
| Box of Smoke (NVIDIA, 2006) | [box-of-smoke](https://github.com/arkits/box-of-smoke) | https://box-of-smoke-port.pages.dev |

The gallery itself lives at [arkits/gpu-demo-gallery](https://github.com/arkits/gpu-demo-gallery).

`scripts/build.sh` assembles `dist/`, taking pictures from the sibling
repositories: rendered, or left by their tests. `scripts/deploy-cloudflare.sh` publishes it. To add
a demo, add a link to `web/index.html` and a picture to `scripts/build.sh`. Pictures show
the demos' content, so they stay out of Git. Fonts come from Google Fonts.
