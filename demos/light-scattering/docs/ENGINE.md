# How Light Scattering works

Addresses are in `App.exe` (an MFC program, 335,872 bytes, April 2002), decompiled with REA
and Ghidra and never run. [I] marks inferences. The demo ships its shaders as source:
`ScatterVS10.vsh`, `SkyPS10.psh`, `TerrainPS10.psh` (and `TerrainPS14.psh`, the same in
ps.1.4); the `...a` files are the same after the preprocessor, with register numbers.
`shaders/demo.wgsl` follows them instruction by instruction.

## Scene

- **Ground** (0x404330, 0x4044d0): `hf129_3.raw`, 129 by 129 bytes; a point every 20 units,
  its height the byte. One triangle strip; the port uses two triangles a cell.
- **Sky** (0x404da0, 0x4050b0): a dome 2580 across and 2000 high, 128 slices by 64 rings,
  reaching 120 degrees down from its top. It is drawn around the camera, 200 below
  the ground's zero (0x406dc0).
- **Order** (0x404ec0): no culling, depth test on; the sky without writing depth, then the
  ground. Bilinear filtering with mipmaps on every stage.
- **Camera** (0x403580, 0x403e70): `D3DXMatrixLookAtLH(place, place + direction, up)`,
  `D3DXMatrixPerspectiveFovLH(0.78, 2.2, 1, 50000)`: the original view is 2.2 times as wide as
  high. The port keeps that view where the window is narrower, and the height where wider.

## The vertex program (0x406dc0)

| c | Holds |
|---|---|
| 0, 4, 8 | World-view-projection, world-view, world |
| 12 | Sun direction (0x406340): (cos φ sin θ, cos θ, sin φ sin θ) |
| 13 | Ground reflectance (0.0138, 0.0113, 0.008) |
| 15 | Sun colour, intensity in w |
| 16, 18 | Texture coordinates from x and z: 1/2560 for the ground, the same plus a moving offset for clouds |
| 22 | Eye |
| 23, 24 | 1 / (βR + βM), βR + βM |
| 25 | y: inscattering multiplier |
| 26, 27 | β'M, β'R |
| 28 | (1 − g², 1 + g, 2g): the program has 1 + g where the formula has 1 + g², kept |
| 29, 30 | βM, βR |
| 31 | (1, log₂ e, 0.5, 0) |

Distance is the depth in the view. Extinction `e^(-(βR+βM) s)` times reflectance and sun goes
out as the first colour, inscattering as the second; both are kept between 0 and 1.

Coefficients (0x403200), for 650, 570 and 475 nm: βR = 1.2444614e-28 / λ⁴,
β'R = 7.4273315e-30 / λ⁴, βM = 3.5407335e-15 K / λ² with K = (0.685, 0.679, 0.67),
β'M = 5.6352525e-16 / λ². The Rayleigh and Mie sliders multiply them.

Sun colour (0x4063a0, turbidity 2 from 0x4064e0): Preetham's transmittance,
`exp(-m 0.008735 λ^-4.08) exp(-m β λ^-1.3)` with λ in µm, β = 0.04608366 T − 0.04586026,
air mass m = 1 / (cos θ + 0.15 (93.885 − θ°)^-1.253).

## The pixel programs

The ground (0x4069a0, 0x406a30): stage 0 is `pnh3.dds` when the sun's z is above 0, else
`nnh3.dds`: normals in rgb, stored as (x, z, y), and the horizon in alpha. Pixel constant c1
is (sun x, sun z, sun y, |sun z|). Colour = sat(N·L) × ground × (1 − sat(c1.a − horizon)) ×
(1 − clouds) × extinction + inscattering. The sky is the inscattering alone.

Clouds (`clouds.bmp`) move 0.0025 of the texture each frame, from 0.5, wrapping at 8.

## The movie (0x4049b0, 0x401320, 0x4055e0)

`Movie.dat`: tracks (1), keys (64), a flag, then for each key the frame it falls on and 17
numbers: place, direction, up; Rayleigh, Mie, inscattering and extinction multipliers and g
(read in that order into fields 3, 4, 1, 2, 0 by 0x403440); sun intensity, θ, φ. Keys are 40
frames apart and end at frame 2940. A frame counter goes up once a drawn frame; the port
takes 60 a second [I].

Every number follows a Catmull-Rom spline through four keys. The frame is first turned into
a share of the keys (0x401240), then spread over the spline's n − 3 pieces rather than the
n − 1 gaps, which the port keeps. The original stops at the end; the port starts again.

## Sliders (0x402ff0)

The dialog shows each number as 0 to 100: sun (0x402fc0: 0.5 − θ/π for φ ≥ π, else
0.5 + θ/π), intensity, Rayleigh, Mie, g and inscattering times 100. The extinction
multiplier (a check box) reaches c25.x, which no shipped program reads. Moving a slider here
pauses the movie [I: the original's playback would set them again].

## Differences from the original

- The view keeps the original's width or height, as above, at any window shape.
- Four samples a pixel.
- Dragging and the wheel turn and move the camera; `,` and `.` move the sun. Not in the
  original, nor is starting the movie by itself or playing it again.
- The dialog's other sliders (separate θ and φ, step size) are left out.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
