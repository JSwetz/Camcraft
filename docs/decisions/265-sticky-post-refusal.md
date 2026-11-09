## #265 - a refused post keeps its reason on screen

- **Chosen:** A refused post still writes "post refused: <reason>" to the
  status line, and it also keeps the reason in `State::post_refusal`. A
  "Post refused" window (`egui_app/post_refusal.rs`, an in-app
  `egui::Window` like the G-code viewer, closed by its ✖) shows the
  reason and the file that was not written. The window goes when the user
  closes it, when a later post succeeds, or when the document is switched
  (New, Open). An edit or regenerate after the refusal does not close it;
  the window adds "The document has changed since this post was refused -
  post again to check it."
- **Why:** The status line is one shared line. The next viewport click
  (`interact::pick_model`), face or edge pick, or command writes over it,
  so the reason was gone before the user could act on it. A post refusal
  is the reason the machine has no program, so it has to stay readable
  until the user is done with it. The window draws nothing while no
  refusal stands, so no shell snapshot moves.
- **Considered and dropped:**
  - A sticky status line (refusals not replaced by ordinary status):
    every other status would then be hidden behind an old refusal, and
    the line is too short for a gate reason that names several moves.
  - Closing the window on any document edit, the way the op page's
    refusal banner retires (`crate::feedback`): the user's fix is an
    edit, so the reason would vanish while they work on it. The
    changed-since note keeps it readable without presenting it as
    current.
  - Putting the refusal in the G-code viewer: the viewer holds the last
    program that was written, and a refusal leaves that program on disk
    unchanged, so mixing them would make one window describe two posts.
  - PR #232's status-strip alert styling is separate: it makes a refusal
    stand out while it is on the line, and this keeps it after the line
    moves on. The two do not conflict.
- **Cost to change later:** Small. One `Option` field set in
  `State::post_program_path` and cleared in the two document-switch
  paths, plus one window function. Other refusals (export, import) can
  reuse the window by generalising the title.
