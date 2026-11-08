## #32 - a headless binary per crate (M13.3)

- **Chosen:** one small `src/bin/<package-name>.rs` per crate (Cargo's
  auto-discovered bin, no `[[bin]]` table, no new dependency), shaped like
  `post`: a `USAGE` const, `-h/--help`, `--version`, hand-rolled argument
  parsing, a `die()` that prints `<bin>: <message>` to stderr, and
  `ExitCode` (0 ok, 1 bad input). What each does, from its crate's public
  API only (layering holds: none touches app or ui):
  - `camcraft-geometry <model.stl>`: binary/ASCII STL -> triangle count,
    bounds, size, degenerate facets, open edges (`repair::open_edge_count`).
  - `camcraft-cl <file.cl>`: CL dump -> move counts by kind, arcs, cycles,
    rapid/cut length, bounds, chaining; non-zero on a broken chain.
  - `camcraft-tools list | speeds <work> <tool-mat> <D> <z>`: the built-in
    catalog with its sanity check, and the cutting-table rpm/feed window.
  - `camcraft-sim <file.cl> <box> <tool-D> [--ball] [--cell]`: dexel cut of
    a box, removed volume, and a verdict; exit 2 when flagged (a rapid that
    travels sideways or descends below the stock top in the footprint, or a
    cut below the stock bottom).
  - `camcraft-motion face <rect> <D> <stepover> <cut-z> <clear-z>`: the
    facing generator over a rectangle, written as a CL dump (stdout or -o).
- **A CL text reader/writer moved into the cl crate** (`cl::text`): the
  golden grammar had a writer only in `crates/app/tests/common` and a
  reader only in post's `.cnc` parser - neither reachable from cl, sim or
  motion. `to_text` is byte-identical to the golden writer; a test reads
  every app golden and writes it back byte for byte. The app's test writer
  is left as is (unifying it touches every app suite's helper; a follow-up
  can make it call `cl::text::to_text`).
- **STL reader in the geometry bin, not the crate:** the importers live in
  app by design (the crate note keeps that seam); the bin carries a small
  reader of its own (header/count/length check, 50 M triangle ceiling).
  STEP was dropped - it needs truck, which geometry must not take.
- **The sim verdict is a CLI check, not the verifier:** the real envelope
  scan needs a part and a tool assembly; the CLI states its simpler rule in
  `--help` and exits 2 on it. It never claims a path is gouge-free.
- **Packaging: not shipped.** `scripts/release.ps1` stages an explicit list
  (`camcraft-ui`, `camcraft-cli`, `post`); the new bins are development and
  test tools and stay out of the zip and installer. `cargo build --release`
  now also builds them (small; seconds). Adding them later is one entry in the sign and
  stage lists of release.ps1.
- **Considered and dropped:** a shared CLI helper crate (five 100-line bins
  do not earn a crate); clap (a new heavy dependency); a JSON spec for
  motion (args cover the one generator exposed; more generators can add
  subcommands).
