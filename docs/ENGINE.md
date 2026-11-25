# How Grove works

Addresses are in `grove000419.exe`, decompiled with REA and Ghidra and never run. [I] marks
inferences.

- A tree is a trunk 1 high that ends in two smaller copies of itself (0x401200). Sizes are
  √(1 - balance) and √balance, plus character × (0.3 × chance - 0.1), kept to 0..1. They lean
  by spread × balance × 90 degrees and the opposite, about an axis that turns by twist with
  each part. Parts shorter than (1 - fullness)⁴ × 0.47 + 0.03, or 31 deep, are leaves
  (0x4116f0). Chances are kept in a tree of their own, so that sliders do not reshuffle them
  (0x404590).
- A branch has six sides and two rings, at the branch size from either end; bands join them to
  the next branches. A leaf is a quad `leaf size` to each side and twice as long (0x406600),
  with random colors from half to full at its corners, cut out above half alpha.
- Each tree has up to eight fireflies that wander about (0, 0, 4) above it (0x404740) and
  are its lights: 1 / (d + 0.1 d²), at corners, no ambient. Their hue goes around; saturation
  0.4.
- The ground is drawn once for each tree, added, lit by its fireflies (0x4054d0): 20 by 20
  quads that fade out from half way to the side. It is flat, with normals of a bumpy one
  (0x404e00).
- The sky is a cylinder of 24 sides with the night picture at half brightness, and a dark lid
  (0x40cc10, 0x40d030). `sky_day` is in the program, but nothing turns the day on.
- The eye: `frustum` half as high as far; flies at 0.1 gained each frame, 0.9 kept (0x411fa0).
  When nothing is pressed it turns to the middle.
- `g` plants fifteen trees, one in each frame, at up to 31.5 from the middle, at least 2 apart
  (0x4135e0). They copy the first tree's numbers.

## Differences from the original

- 60 steps a second [I: steps and measured time are mixed].
- The panel keeps its shape in wide windows and is higher up. Its order and looks, other than
  the labels, are inferred [I].
- Up to 60 trees. Leaf colors are the same each time a tree is built.
- Flickering edges of fireflies change with the simulation, not with each picture.
- Left out: help, statistics, `n`, `p` (an unlimited ground) and the built-in benchmark.
- Scrolling is new.

Not checked: side by side with the original, Safari, Firefox, phones, Windows, Intel Macs.
