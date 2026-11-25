# Zoltar engine notes

The port uses the original scene data and static analysis of `Zoltar.exe` with Ghidra through
`npx -y rea-agents@6.10.1 decompile-all`. The second-generation viewer's Scene root names
its materials, textures, geometry, transforms, animation and render groups. Geometry uses
typed vertex streams and 16-bit indices. The loader checks the complete binary framing.

Each speech scene supplies timed vertex-position and normal arrays plus named transform channels. The port loads one clip at a time and interpolates its mesh snapshots. The voice offsets come from `fortuneAnimations.txt`, `greetingsAnimation.txt`, and `idleAnimations.txt`. The browser rejoins the numbered asset chunks before verifying and unpacking them.

The port approximates the light setup and combiner math. The original random fortune order, automatic idle prompts, clickable gem and draw menus are not reproduced.
