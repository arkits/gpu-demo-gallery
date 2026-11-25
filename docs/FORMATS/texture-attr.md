# Texture attribute sidecar (observed)

Status: **Observed in apilib decompilation**.

## Evidence

- `.rea3dfx/apilib/functions/0x1003dfe0.c` reads a 0x1fc-byte header, byte-swaps fields, checks magic `0x5444521f` (`'TDR\x1f'` as stored in the checked field), and reads attribute chunks.
- This path is reachable from apilib's image/texture handling code.

## Notes

- The original ATBRT package does not document this format.
- The exact chunk schema is not fully mapped here. A reader should validate size, magic, endianness conversion, chunk bounds, and reject unknown layouts.
