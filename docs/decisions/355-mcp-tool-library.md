## #355 - MCP: export and import a tool library

- **Chosen:** two MCP tools, `export_tool_library {path}` and
  `import_tool_library {path}`, each a re-dressed script verb
  (`cam.exportTools(path)` / `cam.importTools(path)` in the VERBS table
  and `assets/scripts/cam.d.ts`), the same shape `open_project` /
  `save_project` already take over `cam.load` / `cam.save`. One meaning
  per verb across the batch and MCP lanes (the #128 design); the verbs
  cost two table rows and two functions in `script/api.rs`.
- **No second format or rule set:** export is
  `tool_library::export_tool_library` plus a file write; import is
  `parse_tool_library` + `ImportTools` (#300/PR #348) executed through the
  session History, exactly as the Tool library page does it. So the
  duplicate skip, the "Name (2)" rename, the refusal of a malformed or
  insane file naming the tool, the refusal of an all-duplicate library
  ("nothing to import") and the one-undo-step law are #348's, not
  restated here.
- **Import reply:** `{path, added, renamed: [{from, to}], skipped,
  summary}` - `ImportReport` as data, with the GUI's status line as
  `summary`. Renames are objects rather than `[from, to]` pairs so an
  agent reads them by key. Export replies `{path, tools}` (the count).
- **Paths:** taken as given (relative to the server's working
  directory), the same as every other MCP file tool (`open_project`,
  `import_model`, `post`). An unreadable file refuses with "cannot read
  <path>", an unwritable one with "tool library not written to <path>".
  Dropped: resolving relative paths against the open document's folder -
  no other tool does it, and one tool doing it alone would surprise.
- **Stacked on PR #348** (#300): the commands and format live there, so
  this branch is based on `issue/300-tool-library-io` and its PR says
  "stacked on #348".
- **Known #348 behaviour kept, not changed here:** duplicate detection is
  by name, so re-importing a library whose clash was already renamed
  ("B (2)") renames it again ("B (3)") rather than recognising the copy.
  Changing that is #348's (or a follow-up's) call.
- **Cost to change later:** low - the MCP tools and the verbs are thin
  wrappers; a reply shape change touches `v_import_tools`, `cam.d.ts` and
  the pinning tests.
