## #352 - Show the build identity in the toolbar and Debug info

- **Chosen:** embed the UI crate version, UTC compilation time, and source
  commit in the executable. The toolbar shows version and source ID on its
  top line, with the UTC build time directly below at the right edge;
  Debug info shows the full commit and build time for bug reports.
- **Why:** screenshots then identify the running binary, and copied Debug
  info can identify its exact source. A source archive without Git metadata
  says `Custom Build`; its packager may set `CAMCRAFT_SOURCE_SHA` to supply
  the exact commit. `SOURCE_DATE_EPOCH` can pin the time for reproducible
  builds. A checkout with local changes adds `-dirty` to the full ID and
  `*` after the short ID, so a test build never claims to be the exact
  committed tree.
- **Dropped:** placing this in the native window title, whose alignment is
  controlled by the operating system, and using the source commit time as a
  substitute for the build time.
- **Later cost:** a different label format changes the toolbar's pixel
  snapshots. The test build uses fixed metadata so those snapshots remain
  repeatable across build machines and times.
