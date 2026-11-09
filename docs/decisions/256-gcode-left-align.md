## #256 - the G-code viewer lists lines from the left edge

- **Chosen:** Each program line is drawn in a left-to-right box exactly
  the listing's width and one monospace row tall
  (`ui.allocate_ui_with_layout` with `set_min_size`), instead of
  `ui.add_sized`. The text starts at the left edge, and every row is still
  exactly `row_h`, so the scroll-cursor law (the middle row of the clip
  rect is the published line) is unchanged. A line wider than the window
  is truncated on its one row; egui shows the whole line on hover.
- **Why:** `add_sized` lays its widget out in a centred-and-justified
  layout, so each line's text was centred in its row: a short `M30` sat
  114 px right of the motion line above it. It also let a line wider than
  the window wrap onto a second row, twice `row_h` tall, which put every
  row below it off the cursor arithmetic.
- **Considered and dropped:**
  - `Label::halign(Align::LEFT)` inside `add_sized`: in egui 0.36 the
    label is allocated at its text's size and that rect is centred, so the
    text stays centred.
  - A horizontal scroll bar for wide lines (`ScrollArea::both`): it
    changes the viewer's geometry that the cursor tests pin, for lines
    that are rare in posted programs. Truncation with the hover is enough.
  - Syntax colouring (the issue's second, "see about" ask) is not in this
    change. It needs new text colours per word class that pass the
    contrast law on both built-in themes and every colour-vision mode, and
    possibly new theme-file keys - an owner decision, left on the issue.
- **Cost to change later:** Small. The row is one call in
  `pages::gcode_listing`; colouring would replace the row's `RichText`
  with a `LayoutJob` without touching the row geometry.
