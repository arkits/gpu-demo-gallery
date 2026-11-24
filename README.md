# GPU demo gallery

One page that links the ports of old graphics card demos:

| Demo | Repository | Site |
|---|---|---|
| Principles of Shading (NVIDIA, 2000) | `principles-of-shading` | https://principles-of-shading-port.pages.dev |
| Creature (NVIDIA, 2000) | `creature` | https://creature-port.pages.dev |
| Car Paint (ATI, 2002) | `car-paint` | https://car-paint-port.pages.dev |
| Pipe Dream (ATI, 2003) | `pipedream` | https://pipedream-port.pages.dev |
| Box of Smoke (NVIDIA, 2006) | `box-of-smoke` | https://box-of-smoke-port.pages.dev |

`scripts/build.sh` assembles `dist/`, taking pictures from frames the demos' tests
rendered in the sibling repositories. `scripts/deploy-cloudflare.sh` publishes it. To add
a demo, add a link to `web/index.html` and a picture to `scripts/build.sh`. Pictures show
the demos' content, so they stay out of Git. Fonts come from Google Fonts.
