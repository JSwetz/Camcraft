## #300 - Tool library export / import (native format)

**Decided:** the document's tools export to a CamCraft-own tool library
file and import back from it, as one undo unit.

- **Format.** `{"format": "camcraft-tools", "version": 1, "units": "mm",
  "tools": [{"name", "definition"}]}`, extension `.camtools`. Each
  `definition` is exactly the tool object the MCP server's `get_job` /
  `add_tool` speak and a `.camproj` stores (`project::tool_to_json` /
  `tool_from_json`), so there is one field mapping, not two. The reader
  also accepts the bare `[{name, definition}]` array MCP lists, so an
  agent's output can be saved straight into a library. Why the envelope:
  a `format` tag tells our file from any other `.json` (a theme, a vendor
  library) with a named refusal instead of a confusing field error, and
  `version` lets the shape change later without guessing. `units` is
  always `mm` (the document's storage unit, whatever the display units);
  any other value is refused rather than silently scaled.
- **Required fields** are the project file's: `kind`, `diameter`,
  `corner_r`, `flute_len`, shank and holder sizes, `material_group`,
  `rpm`, `feed`; the later additions (`tip_angle`, `pitch`, ...) default
  as they do in an older `.camproj`. A missing field refuses by tool
  name.
- **Whole-file validation.** Malformed JSON, a wrong tag or version, a
  nameless tool, a name twice in the file, an unparsable definition, a
  tool `Tool::is_sane` refuses, or a name reserved for setup furniture
  (`Stock`, `G54`...) refuses the WHOLE import, naming the tool, with
  nothing applied. Why: a library that is half-wrong is better fixed and
  re-imported than half-applied with a list of skips the user may not
  read.
- **Duplicates never overwrite.** A name the document already uses: an
  identical tool is skipped (it is already there); a different tool comes
  in renamed with the first free ` (2)`, ` (3)` suffix, so every
  operation keeps exactly the tool it had. The status line reports what
  was added, renamed and skipped. A library whose every tool is already
  present refuses ("nothing to import") rather than pushing an empty
  undo step.
- **Tool numbers are not exported** (#221 numbers belong to a machine's
  tool table; importing them would collide with the document's own).
  Imported tools come in unnumbered.
- **Export of an empty document refuses** ("no tools to export").
- **UI:** "Export library..." / "Import library..." beside "New tool..."
  on the Tool library page; rfd dialogs like the other open/save doors.

**Considered and dropped:** overwrite-on-duplicate (could silently change
the tool an operation cuts with - the issue's own warning); per-tool skip
on a bad entry (partial imports hide errors); merging tool numbers;
vendor library formats (Fusion `.json`, `.hsmlib`) - Autodesk formats,
out of scope by the owner's clean-room decision, #300 stays open for them.

**Cost to change:** small. The format is versioned; a v2 reader can keep
reading v1. The duplicate policy lives in `ImportTools::new` alone.
