# How Still Life works

Addresses are in `still.exe` as unpacked by `upx -d`. It is the same PowerVR "D3DShell"
framework as KyroToon (see that port's ENGINE.md for tables, tracks and camera).

## Scene (0x403010)

30 frames a second, 600 in a loop. Two lights at 0x49bab0, a spotlight and a point light; the
spotlight's place and direction have a track for every frame (light tracks at 0x49f3c8: place,
direction, colour, on). 34 meshes at 0x508f80, 14 materials at 0x49b650 (each one diffuse map),
camera at 0x49f3e8 with tracks at 0x524b58. The render style stays 0 (the menu has none).

## The frame (0x416100)

1. Clear to black, depth and stencil.
2. The window (0x440bc0): a rectangle, centre (0, 0) and half size 0.5 of the screen, drawn
   white with identity transforms and stencil REPLACE 1.
3. Each mesh in turn, twice (0x4241d0):
   - where the stencil is not 1 (0x4413d0): mode 0, the diffuse map times D3D lighting
     (white material, ambient from the material);
   - where it is 1 (0x441330): ProcessVertices, then each vertex's colour becomes black with
     alpha 255 − (r+g+b)/3 of the lit colour (0x41b470); mode 0x34 (0x43b170).
4. The overlay picture over the window, alpha blended: a ragged paper edge and the ST logo.

Mode 0x34: texture factor white; stages 0 to 5 each BLENDCURRENTALPHA their hatching picture
over what is there (white at stage 0), and make the next alpha with ADDSIGNED2X of the vertex's
darkness and the picture's alpha. Each picture's alpha is one level, so a darker vertex lets
more pictures through. Stage 6 multiplies by the silhouette picture (|N·V| as in KyroToon).
All six use the diffuse coordinates.

The hatching pictures are named in `hatch.ini` ("Set Hatching"); each is "Set Texture ...
MipMap" and loads its smaller pictures, `Tam_250_256.bmp` down to `_32`, as its mipmaps
(0x442ee0). Filtering is linear with the nearest mipmap on stages 0 to 5 (0x416970).

## Controls (0x415190)

Arrows move the window by 0.01 within the screen; I and D grow and shrink it; A makes it bounce
(0x417170: gravity −9.8, steps of 1/75 s, reversing at the edges); C shows the credits.

## Left out

The menus, text windows and the free camera (F).
