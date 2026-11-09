## #258 (part 1) - the Post dialog remembers where the document posted

- **Chosen:** The Post dialog opens in the folder of this document's
  last successful post, proposing that same file name; otherwise in the
  saved project's folder, proposing `<project name>.nc`; otherwise with
  no folder and `program.nc`, as before. The rule is a pure function
  (`ui::post_file::post_target`) the dialog reads, so it is tested without
  a native dialog.
- **Why:** The dialog opened with no folder and `program.nc` every time,
  so every post meant walking back to the job's folder and renaming, and
  re-posting after an edit meant finding the old file again. "Change /
  remember the output folder" is #258's first ask.
- **Where "last post" comes from:** the G-code viewer's `PostView::path`,
  the file the last successful post wrote. It already retires on New and
  Open, so a new document never inherits another one's folder. A refused
  post writes nothing, so it does not move the remembered file.
- **Considered and dropped:**
  - The project name winning over the last post's name: re-posting after
    an edit is the common case, and the last file is the one the shop
    loads, so proposing it again saves a rename. The project name is the
    fallback for a document's first post.
  - Saving the output folder in the project file: it would put a
    machine-local absolute path into a shareable document. It's per
    session, like the viewer.
- **Cost to change later:** Small. One function and the dialog's two
  setters.
