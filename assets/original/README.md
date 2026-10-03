# Original SimTower resources

These assets were extracted from the company-owned Windows executable with:

```sh
cargo run -p simtower-inspect -- \
  analysis/input/SIMTOWER.EXE extract assets/original
```

The source executable SHA-256 is
`2825a3c53f77945c63b6d72e26faa7dde5ddd56c31ca668e67a12576d7feca96`.

- `bitmaps/`: NE `BITMAP` resources normalized to 32-bit top-down BMP.
- `icons/`: the two original NE `ICON` resources normalized to transparent BMP.
- `sounds/`: 58 RIFF/WAVE resources trimmed to their declared RIFF size.

The extractor is deterministic. Numeric filenames preserve the original NE
resource identifiers so Ghidra findings and Rust runtime mappings stay aligned.
