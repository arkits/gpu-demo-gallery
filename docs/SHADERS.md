# Shaders

`.ssh` files are text: variables, textures, vertex streams, then `StartShader` blocks with
properties and passes. `crates/sushi-shader` parses all 35 and translates each pass.

## Language rules found the hard way
- A pass starts from the state of the pass before it. `Inherit(this, shader, pass)` takes
  a finished pass; `Inherit("file.ssi", ...)` adds another file's constants.
- **Constants apply in statement order.** A matrix covers four registers, and a later
  matrix at c4 must hide an older vector at c5. Applying them by register number broke
  the marimba glow.
- The engine's default is `Cull CW` (`gDefault.ssh`), the opposite of D3D's default.
- `;` and `/* */` are comments too, and `//` hides a block-comment marker.
- `#replicate ($i, start, count, step) ($j, start, step)` repeats its body.
- A shader matches a draw when it has every property asked for: the script's property
  (`Normal`, `Shadow`), `FastRT<n>` for n lights, and `Glow` or `Vibrate` when active.

## Translation
- `cN` become `array<vec4<f32>>` uniforms; `def` values are inlined. `c[a0.x]` indexes the
  same array, which holds the 40-matrix palette at c30.
- Matrix constants hold rows, for `dp4`. FastRT matrices are four independent vectors.
- `rcp` and `rsq` of zero give a large number; `pow` uses the absolute value.
- ps.1.4: `texld rN` samples stage N, `_dw` divides by w, `phase` and `+` are ignored.
- Color interpolators are clamped and centroid-sampled, as on the original hardware.
- `D3DCOLOR` attributes are BGRA bytes.
- Passes without shaders (the shadow quad) use a small fixed-function equivalent.
