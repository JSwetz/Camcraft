## #441 - Stock page shows stock only; Theme gets its own ribbon door

**Decided:** `stock_page` ends at its action row. Machine and Post are
edited only on their setup nodes (`setup_pages.rs` still calls
`pages::machine_section` / `pages::post_section`, so both stay). The
theme section is gone from the pages; a new `Cmd::Theme` ribbon cell
("Theme…") stands right of Panels in the View group and opens a
hand-drawn dropdown (`egui_app/theme_drop.rs`) listing the same rows
`State::theme_options` gave the old combo - Dark, Light, the discovered
`assets/themes/*.json` files by name, Browse... - followed by the
colour-vision modes. Picks queue `SelectTheme` / `SelectThemeDialog` /
`SelectColorVision`, so undo and redo hold; opening the door is view
state like the other doors.

**Why:** the issue asks for exactly this; the hand-drawn dropdown is the
law the Machine and View doors already follow (foreground Area, outside
press closes, one door open at a time), so the tests click rows through
the same geometry the ribbon draws.

**Colour vision moved with the theme.** The colour-vision picker
(M13.4) rode inside the theme section; removing the section from the
Stock page would have left it nowhere. It is part of "the document's
look", so it joins the Theme door's list under a "Colour vision"
caption. `colorvision::picker` (the old combo) was deleted as unused.

**Dropped:**
- The separate "Built-in" reset button the old section showed for a file
  theme: the list's Dark row queues the identical `SelectTheme(Dark)`,
  so a second row would be a duplicate.
- An icon for the door: no glyph in the bundled Tabler set fits a theme
  or palette; the cell follows the View door (a list door captioned by
  its own word, `icon_of` None). Adding a Tabler `palette` PNG later is
  a one-line `icon_of` change plus the asset and its notice.
- A real egui `ComboBox` inside the door: a popup inside a hand-drawn
  Area fights the outside-press close rule.

**Cost to change:** small - the door is one module and one `Cmd` arm.
