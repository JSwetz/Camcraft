# CamCraft 0.8.0 - hand-test checklist

About 12 checks, roughly 30 minutes, aimed at what changed since 0.7.0. For
each, do the steps and compare with **Expect**. Anything that differs is worth
a note: a screenshot and one sentence is plenty.

Start `camcraft-ui.exe`. Import a part (Model), set Stock, add a tool (Tools).

1. **Facing around a hole.** Face a plate with a through-hole. **Expect:** the
   rows stop a tool radius short of the hole and lift over it; *Holes: Through*
   gives the old straight rows.
2. **Cutter compensation.** Contour with a straight lead-in, set
   *Compensation: In control*, post with the Fanuc post. **Expect:** `G41` or
   `G42 D..` on the lead-in and `G40` on the lead-out. With an arc lead-in the
   operation refuses by name.
3. **Adaptive arcs.** Generate a 2D Adaptive pocket and open the G-code.
   **Expect:** G2/G3 moves on the spiral; no warnings.
4. **3D Pocket, Adaptive.** Set *Strategy: Adaptive* on a 3D Pocket over a part
   with a boss. **Expect:** spiral levels; a level that cannot be planned says
   "Offset rings cut the level at Z ...".
5. **Two models, one contour.** Place two blocks edge to edge, pick the outer
   edges of both. **Expect:** one ring, cut at every depth.
6. **Repair mesh.** Import an STL with a missing face, Model page → Repair mesh.
   **Expect:** a report ("filled 1 hole ..."); Ctrl+Z restores the original.
7. **Machined stock.** Simulate a program to the end. **Expect:** the machined
   stock is drawn and hovering it highlights the stock.
8. **Playback jump.** Drag the playback slider to the end of a long program.
   **Expect:** the stock updates about once a second, no freeze.
9. **Lathe.** Mount a lathe machine. **Expect:** the Setup pane offers the
   *Fanuc Lathe* post; Drilling appears on the ribbon with Drill only; an axial
   drill posts with `G83 Z.. R.. Q..`.
10. **3+2 boundary.** Index an operation (3+2) and give it a boundary offset.
    **Expect:** it generates; the preview grows in the tilted plane.
11. **Setup sheet.** Print the setup sheet with a fixture marked Fixed.
    **Expect:** "fixed (machine frame)" on that line, "turns with the part" on
    the others.
12. **Installer** (optional). Run `camcraft-0.8.0-win-x64-setup.exe` if it is
    in the release. **Expect:** installs per-user without an admin prompt.
