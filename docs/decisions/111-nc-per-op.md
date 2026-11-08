## #111 - one NC file per operation (2026-11-07)

- **Chosen:** a post option "Output" on the setup page, next to the Post
  picker: "One program" (default) or "One file per operation". It is a
  second field on the `PostOptions` singleton (`per_op`), switched by the
  same undoable `SetPostOptions` command, saved as a top-level
  `per_op_files` key written only when on (older files load one
  program; no version bump). The combined program is byte-identical, so
  every golden stays.
- **Files:** `<program>_<nn>_<op>.<ext>` into a folder the Post dialog
  asks for (a folder picker replaces the save dialog when per-op is on).
  `<program>` is the project file's stem, `program` when unsaved; `nn`
  is the 1-based position in program order, two digits at least; both
  names are cut to ASCII letters, digits, `-` and `_` (runs collapse,
  48 chars max). The op name is the entity name, unique, so no clash.
  The extension is the script's own manifest `extension` (`nc` when it
  states none).
- **Standalone by construction:** each op's own stream (the records the
  combined program is joined from, `posting::op_streams`) posts alone
  through the same script, so every file gets the dialect's whole-program
  shape - header and units, the op's tool change, the safe-start block
  (G90, work offset, G40/G49/G80), spindle and coolant for that op, and
  the dialect's end (coolant off, M5, home, M30). The retract on entry is
  the op's own clearance approach after the tool change, exactly as the
  first op of a combined program starts; no extra word was added, so a
  per-op file of op 1 starts like the combined program.
- **O-numbers:** the engine publishes `programNumber` before `onOpen`
  (1 for a combined program); Fanuc, Mazak and Fanuc Lathe write
  `O0001`, `O0002`... per file. There is no job program-number setting
  yet, so the base is the `O0001` every program already wrote. Haas,
  Siemens and Heidenhain write no O-word today and are unchanged.
- **Gate:** one refusal refuses all. The program gate (freshness, tools,
  gouge, limits, fixtures, probe) runs once over the same op list; each
  op's post must come back with no counted warning; only then is
  anything written - into a scratch folder in the target, then renamed
  into place; a failed rename removes the files already moved.
  Suppressed ops write no file.
- **Considered and dropped:** deleting older `<program>_NN_*` files a
  previous, longer run left in the folder - removing files the user may
  have kept is worse than a stale extra file, and the numbers make the
  set plain. Judging each file by its own material sim (fresh stock):
  the gouge law is against the model, so the whole-program sim already
  judges every op's cuts; a rerun on a part already cut is the use case.
