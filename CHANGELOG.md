# Changelog

One row per behaviour-changing commit: `commit | date | change | version`.
Rows are written at each cut from the merged PRs' `Changelog:` lines
(AGENTS.md "Issue Law"); older `Unreleased` rows took the version at their
cut. Issue references are `(#N)` at the end of the change. Docs-only and
release-mechanics commits are not listed; history before 0.7.0 is in
`RECOMMENDED.md` and git history (the retired `DONE.md` is in every tag up to
v0.8.1).

| Commit | Date | Change | Version |
|---|---|---|---|
| f4e4eac | 2026-11-08 | fix: idle frames no longer redraw and window resizing stays responsive; rotated face and edge picks no longer freeze; with a single part (or its named source model) a move or rotation carries the stock along in the same undo step (#164, PR #239) | 0.14.0 |
| 919f2fd | 2026-11-08 | feat: Probe operation - face, edge, outside-corner and bore/boss-centre finding with a touch probe - G31 skip touches on the Fanuc post with a no-contact alarm, set G54-G59 or stop for the operator, out-of-tolerance stop; posts that cannot probe refuse it by name (#108, PR #178) | 0.14.0 |
| 5208e94 | 2026-11-08 | feat(post): new Tormach PathPilot post and Tormach 770MX machine; a machine file in inches now posts inch (G20) programs through posts that read `machine.units` (#131, PR #162 by mkinney) | 0.14.0 |
| 501d012 | 2026-11-08 | feat(ui): the origin triad is drawn chunkier (3 px, outlined) and shorter, with a brighter Z and larger axis letters (#195, PR #234 by bherbruck) | 0.14.0 |
| a74690e | 2026-11-08 | feat: tools can carry their controller tool number (tool editor), which the post writes as T/H; unnumbered tools still post as their list position, and two tools posting one number are refused (#221, PR #244 by mkinney) | 0.14.0 |
| b99269d | 2026-11-08 | feat: align a work offset 3-2-1 from a picked flat face, straight edge and vertex or bore (Align WCS button under the picks) (#187, PR #201 by bherbruck) | 0.14.0 |
| 7f51ce4 | 2026-11-08 | fix(motion): A single-band bar with a D-flat fits its axis again (regressed in the point-concurrency guard) (#246, PR #249 by bherbruck) | 0.14.0 |
| 12ad432 | 2026-11-08 | feat(post): fanuc Lathe post can write threads as a two-block G76 cycle (new post option "Threading: G76 cycle / G32 passes", default G32), from a thread-cycle record the threading toolpath now carries (#34, PR #211) | 0.14.0 |
| 2f3dd2b | 2026-11-08 | fix(motion): lathe Facing faces the stock's allowance down to the part's end instead of reporting nothing to face (#122, PR #199 by bherbruck) | 0.14.0 |
| 0a13f96 | 2026-11-08 | fix(motion): cross-drill station changes with off-centre holes hop clear of the bar instead of being refused (#81, PR #209 by bherbruck) | 0.14.0 |
| 53fb4da | 2026-11-08 | feat(ui): create cells darken with no stock in the setup, and a click opens the Stock page (#216, PR #242 by bherbruck) | 0.14.0 |
| 6a4a7f9 | 2026-11-08 | fix(motion): the axis fit no longer trusts a cross axis for a finely meshed ball with a flat rod or bar through its centre (#237, PR #247 by bherbruck) | 0.14.0 |
| 9bb8d26 | 2026-11-08 | fix(app): new operations post after their setup's existing ones in creation order, not in name order (#167, PR #233 by bherbruck) | 0.14.0 |
| 3a0b564 | 2026-11-08 | feat(ui): the Stock page shows the models' size and X/Y/Z extents (#197, PR #235 by bherbruck) | 0.14.0 |
| a859847 | 2026-11-08 | feat: box stock can be fitted to the models with a separate allowance per side (-X, +X, -Y, +Y, bottom, top) (#198, PR #238 by bherbruck) | 0.14.0 |
| 010bca6 | 2026-11-08 | feat(post): cutter compensation gains a Wear mode: the contour's tool-centre path is posted under G41/G42 Dn / G40 with the register holding only the wear delta (Fanuc; other posts refuse by name) (#38, PR #224) | 0.14.0 |
| 03f5498 | 2026-11-08 | feat(motion): 3D finish refuses a stated stepover or steep stepover at or past the ball diameter, and a tolerance at or past half the ball radius, by name ("the scallop ceiling"), instead of generating a path whose cusp verified silently (#37, PR #218) | 0.14.0 |
| 983b44b | 2026-11-08 | feat(cli): `camcraft-cli mcp`: a stdio MCP server that lets an AI agent open or import a model, set stock and tools, add, edit or delete operations, generate, verify (findings as data) and post - every change undoable (#128, PR #184) | 0.14.0 |
| 51ee576 | 2026-11-08 | feat: operations choose their coolant - Off, Flood (default), Mist or Through tool - and every post writes the mounted machine's own code for it, M9 before each tool change and at program end; a coolant the machine has no code for blocks the post by name (#112, PR #147) | 0.14.0 |
| bbafc23 | 2026-11-08 | feat(sim): the stock view's narrow band is re-normalized by one eikonal (fast-sweeping) pass before the render, so shading reads a true distance gradient; sign and surface unchanged (#22, PR #203) | 0.14.0 |
| 6f5560d | 2026-11-08 | feat(sim): the rotary stock simulation states its flute-rotation bound (RotaryChordStats::flute_shift) and can enforce a flute tolerance (Stock::simulate_rotary_bounded) (#20, PR #189) | 0.14.0 |
| 245fa53 | 2026-11-08 | feat(ui): unsaved changes are tracked: the title shows a leading `*`, and closing the window, Ctrl+Q, New or Open over unsaved work asks Save / Don't Save / Cancel; a cancelled or failed save keeps the app open (#107, PR #141) | 0.14.0 |
| c9a9fb3 | 2026-11-08 | fix(motion): the lathe and rotary axis fit refuses by name a part whose normals only meet at one point (a plate or block with a turned boss, a single-band shaft read across) instead of trusting a wrong axis (#208, PR #223) | 0.14.0 |
| 5294bc3 | 2026-11-08 | fix(motion): the lathe axis reader reads a short turned bar (L/D about 0.5-2) about its true axis from its facing flats instead of refusing it as "every axis fits"; spheres and cubes are still refused (#152, PR #204) | 0.14.0 |
| 0cd9bc2 | 2026-11-08 | fix(app): A boring bar's head and shank are judged on its cutting feeds against the part and the stock still uncut (pilot, ops ahead, its own cut so far); a body that rubs is a ShankHit that blocks the post (#146, PR #185) | 0.14.0 |
| 899d428 | 2026-11-08 | fix(motion,app): turning kinds (profile rough/finish, groove, part, thread, form) approach and retract clear of a setup bar wider than the part, and roughing steps down from the bar's OD - a 20 mm bar can now be roughed to a 12 mm part and posted (#130, PR #190) | 0.14.0 |
| 272fba0 | 2026-11-08 | fix(motion): the rotary wrap refuses by name a work with a non-turned feature across its seam, or a hole in its skin, instead of reporting "the mesh is open"; D-flats clear of the seam wrap as before (#54, PR #210) | 0.14.0 |
| eacc09c | 2026-11-08 | fix(app): cross-hole entry and shank reach read a non-conforming (unwelded) STL round at its true surface, not its chords (#121, PR #179) | 0.14.0 |
| 0aa542f | 2026-11-08 | fix(app): cross drill on box stock: the holder is read along its own footprint on each station (a box at its face), keeping the corner radius only for swings around the bar, so a holder that clears the face is no longer refused (#119, PR #219) | 0.14.0 |
| 48ab1df | 2026-11-07 | fix(sim): sideways rapids under the clearance plane are judged along their whole length, so a link rapid through a narrow uncut wall between two pockets is refused instead of posted (#140, PR #160) | 0.14.0 |
| 8b3163d | 2026-11-07 | fix(motion): lathe axis fit tightens its inlier band to the revolve's own noise, so a round bar with one modelled cross hole reads its true axis within the spindle tolerance and cross-drills clean (#116, PR #168) | 0.14.0 |
| 334cdec | 2026-11-07 | fix(app): an op whose rapid verdict reads earlier ops (prior acquittal, lathe pilot/facing credit) goes Stale when an earlier op of its setup is regenerated, deleted or reordered, and the post refuses it until regenerated; one Regenerate All brings such jobs current in one undo step; one-time: every non-first op (not 3+2-posed) of a saved project reads Stale once after upgrading (#109, PR #153) | 0.14.0 |
| e434170 | 2026-11-07 | fix(build): project files round-trip every float exactly, so ops no longer read Stale after a save and load, and imported mesh vertices no longer drift by one ulp (#169, PR #177) | 0.14.0 |
| 9bc121f | 2026-11-07 | fix(app): A turned path's axial rapids are judged in the meridian plane against the revolved stock (the bore as air, a pilot drill ahead credited); one into the bar blocks the post (#103, PR #135) | 0.14.0 |
| 4020abf | 2026-11-07 | fix(app): A stock edit after generation marks every op that read the stock Stale (rapid-in-stock, prior acquittal, drill and lathe checks), so the post refuses until it is regenerated; saved projects show those ops Stale once (#156, PR #165) | 0.14.0 |
| e09ca3f | 2026-11-07 | fix(ui): pinning or unpinning a ribbon cell can now be undone (Ctrl+Z) and counts as a document change (#143, PR #154 by mkinney) | 0.14.0 |
| ee8e737 | 2026-11-07 | fix(app): A cross (radial) drill is judged against the finished part: a hole the model does not have, or one drilled past the modelled hole's floor, is refused by name and never posts (#117, PR #158) | 0.14.0 |
| d612583 | 2026-11-07 | fix(sim): rapid-in-stock checks read the tool's whole footprint, not only its centre: a dive whose rim meets the stock, or a re-entry whose far side is uncut, blocks the post (#106, PR #138) | 0.14.0 |
| 0574c31 | 2026-11-07 | fix(post): the Heidenhain DIN/ISO post now declares the TNC's own `.I` program extension (#52, PR #139 by mkinney) | 0.14.0 |
| 5accdd5 | 2026-11-07 | fix(app): drilling reads the stock the setup's earlier operations left - a hole on a pocket floor, an R plane on a faced top (mill and lathe axial) and the holder check credit the earlier removal; unknown or tilted removal reads as raw stock; a drill goes Stale when an op ahead of it changes (#89, PR #123) | 0.14.0 |
| 88bde70 | 2026-11-07 | fix(app): cross holes on box stock enter at the face, not the corner radius - no air drilling, and the reach check reads depth from the face (a 30 mm hole in a 44 mm square bar posts on a 35 mm flute) (#85, PR #120) | 0.14.0 |
| 63896a1 | 2026-11-07 | fix(app): lathe cross drills are no longer refused by the axial-hole check ("cuts past the finish surface"); only axial lathe holes get that check (#80, PR #118) | 0.14.0 |
| 329871c | 2026-11-07 | fix(motion,app): the lathe axis fit sets keyways, flats and cross holes aside, so a correctly placed keyed shaft passes the spindle law; a non-blocking note names the share set aside; the refusal gives move-the-offset advice only for a misplaced part (issue #71, PR #115) | 0.13.0 |
| 1eae68e | 2026-11-07 | fix(sim,app): a vertical rapid going down into uncut stock blocks the post, judged against what earlier operations of the setup removed (turning rapids along the spindle axis keep their old reading) (issue #96, PR #105) | 0.13.0 |
| 57b2002 | 2026-11-07 | fix(app): drilling refuses by name an R plane below the part or unfaced stock top - the rapid down to R crossed material and posted silently (issue #91, PR #97) | 0.13.0 |
| f4c0679 | 2026-11-07 | fix(app): drilling refuses a hole deeper than the flute from the stock surface (bores and taps in an existing hole need only a shank that fits) and proves the holder clear of the stock, per Z band, incl. the cross-drill end-face swing (issue #84, PR #90) | 0.13.0 |
| 588bccd | 2026-11-07 | fix(build): the AppImage no longer crashes at startup (PR #102, by linuxguy123) | 0.13.0 |
| 902a1b1 | 2026-11-07 | feat(build): macOS (ZIP, DMG), AppImage, Snap and Flatpak release packages (PR #98, by linuxguy123) | 0.13.0 |
| f8dba8f | 2026-11-07 | feat(build): Linux x86_64 TAR.GZ, DEB and RPM release packages (PR #95, by linuxguy123) | 0.13.0 |
| 0abcdeb | 2026-11-07 | build(aur): Arch Linux source package (PKGBUILD, .SRCINFO) in aur/ (PR #114, by linuxguy123) | 0.13.0 |
| 685b0e7 | 2026-11-07 | fix(sim): the post gate's rapid-in-stock law now judges radial (cross-drill) programs against the raw stock - a rapid through uncut stock is named RapidInStock and blocks the post; retracts up a drilled hole stay clean (#79, PR #83) | 0.12.1 |
| 4fb33d3 | 2026-11-07 | fix(app): cross drill stands off the larger of the raw stock and the part, so no rapid between stations passes through uncut stock; refuses by name any rapid that would; resizing the stock stales the op (#77, PR #86) | 0.12.1 |
| 053e0c7 | 2026-11-07 | fix(app): cross drill measures its clearance and standoffs about the work offset's spindle, refuses a mill-turn bar off its work offset, and refuses on a lathe a part that can't be read as turned (#67, PR #78) | 0.12.1 |
| c50835f | 2026-11-07 | fix(ui): GPU startup tries adapters one by one (named choice, high-performance, discrete before integrated, native backend first), configures the surface from its reported capabilities, adds --gpu/CAMCRAFT_GPU, refuses by name listing every adapter tried, and logs frames over 100 ms by phase (#74 part, PR #76) | 0.12.1 |
| 8a959b1 | 2026-11-07 | fix(app): turning ops (and the lathe axial drill) whose fitted axis is not world Z through the work offset within 0.0005 mm are refused by name at generation and at posting, instead of posting a taper or eccentric (#62, PR #65) | 0.12.0 |
| 7e013e7 | 2026-11-07 | feat(sim,app): turning programs (facing, profile, contour, boring) simulate on the field stock view about their fitted spindle with meridian parity; blade/thread/form tools refuse by name (#19, PR #60) | 0.12.0 |
| 5956e98 | 2026-11-07 | feat(motion): Adaptive clears the rotary developed wrap level by level under the load and kink laws; refused levels named by radius, cut by Offset rings (#9, PR #57) | 0.12.0 |
| a68e17a | 2026-11-07 | feat!: project Save As and kept filenames; operation rename, reorder, copy and paste with order-driven playback; browser collapse and viewport visibility controls; GPU buffer guards - v50 project files, refused by older builds (#45 part, PR #58 by linuxguy123) | 0.12.0 |
| d4dc943 | 2026-11-07 | fix(post)!: Heidenhain DIN/ISO posts the TNC block form; drilling as moves, tapping and non-first offsets refused - breaking post output (#50) | 0.12.0 |
| 80aafb4 | 2026-11-07 | fix(post)!: Heidenhain DIN/ISO XY arcs post their absolute centre in its own I/J block - breaking post output (#49) | 0.11.0 |
| bf92363 | 2026-11-07 | fix(motion,app): Swarf links halved, poses never under level, ball re-entry into its own cut is no plunge (#17) | 0.11.0 |
| c18254c | 2026-11-07 | fix(app,motion): Rotary Offset plunge audit judges against the true cylinder of the bar (#16) | 0.10.0 |
| cf5b210 | 2026-11-07 | feat(motion): Rotary Parallel infeeds and Swarf first entry ramp in under the sloped-entry retry (#15) | 0.10.0 |
| 95881f0 | 2026-11-07 | fix(motion): a straight lead-in outside a round bore is seated inside - one descent per level (#14) | 0.10.0 |
| 3bfed30 | 2026-11-07 | feat(motion): adaptive arc output refits morph-spiral runs as tangent biarcs - a third fewer moves (#11) | 0.9.0 |
| 01c6a28 | 2026-11-07 | fix(motion): the load reader faces a chord plan's curve, not the lagging chord (#10) | 0.9.0 |
| 3b4c35d | 2026-11-07 | fix(motion): each raster pass ends with a rim row where its ground ends - tilted shelf scallop gone (#46) | 0.8.4 |
| 96abbce | 2026-11-07 | fix(motion): the finishing march rides a convex fold edge - coarse-mesh scallops under tolerance (#36) | 0.8.4 |
| 4e27eea | 2026-11-07 | fix(motion): 2D Adaptive refuses a long thin channel by name in a second instead of grinding for minutes (#13) | 0.8.4 |
| 619e479 | 2026-11-07 | fix(sim): Contour moves answer to the part walls along their length; pocket chord law only below the stock top (#43) | 0.8.3 |
| 5083771 | 2026-11-07 | fix(motion): control compensation refuses a contour the controller cannot offset (#40) | 0.8.3 |
| 3c0e2d8 | 2026-11-07 | fix(sim): the gouge law measures every chord of an Area move along its length (#42) | 0.8.3 |
| 0605698 | 2026-11-07 | fix(app): a whole-part Pocket/Adaptive clears the stock around the part, never its own section (#12) | 0.8.3 |
| 3271457 | 2026-11-07 | build(scripts): scripts/gate.ps1 - quick / batch / release tiers and single-test runs | 0.8.2 |
| b6b584b | 2026-11-07 | fix(app,ui): Circular fits its circle to the selection (#4) | 0.8.2 |
| 9318355 | 2026-11-07 | fix(app,ui): 2D Adaptive - close an edge loop, the Selection height offset, Outside containment (#1, #2, #3) | 0.8.2 |
| 64a5708 | 2026-11-07 | fix(ci): the release workflow may write the repo's contents - create the GitHub Release | 0.8.2 |
| c47e2b3 | 2026-11-06 | feat(geometry,motion,sim,app): contour depth reads touching bodies as their union (M13.11) | 0.8.0 |
| 2841e93 | 2026-11-06 | feat(geometry,app,ui): mesh repair - weld by spatial hash, fill planar holes, one undoable command (M13.11) | 0.8.0 |
| ba6e17b | 2026-11-06 | feat(motion,app,ui): facing rows go around holes - Face.holes Lift by default (M13.11) | 0.8.0 |
| f7e96e1 | 2026-11-06 | feat(app): contours picked across two models stitch into one ring (M13.11) | 0.8.0 |
| 6c7abf3 | 2026-11-06 | feat(cl,motion,post,app,ui): G41/G42 cutter compensation for Contour - the part contour under control comp (M13.11) | 0.8.0 |
| abddbaa | 2026-11-06 | fix(motion,sim): the finishing march seeks on a ladder past fold gaps; a closed ring never flanks its own inside (M13.9) | 0.8.0 |
| 6caab50 | 2026-11-06 | feat(app,ui): the area-control leftovers close - ramped clipped re-entries, Trace/Engrave containment, the tilted Face band (M13.10) | 0.8.0 |
| 8c9dfaf | 2026-11-06 | feat(app,sim): the rotary lift - the field eye chords every rotary kind, the column verdict refuses by name (M13.1) | 0.8.0 |
| 66c1860 | 2026-11-06 | fix(sim): the fixture sweep walks the mid-move pose law with a proven swing (M13.8) | 0.8.0 |
| 7205935 | 2026-11-06 | feat(app,ui): the ribbon reads the command door's word; the Setup pane offers the matching post on mount (M13.5) | 0.8.0 |
| e006e49 | 2026-11-06 | feat(motion,app,ui): the Pocket3D port of the HSM planner - the 2D plan inside the 3D level law (M11.4) | 0.8.0 |
| 00c5c99 | 2026-11-06 | fix(motion): late leads re-marked on the re-ordered stock; the island's hops are a leading graze (M11.4 round 6) | 0.8.0 |
| d352e37 | 2026-11-06 | feat(motion): arc output for the HSM adaptive plan under three proofs; the load reader faces an arc's tangent (M11.4) | 0.8.0 |
| 6bef486 | 2026-11-06 | feat(post,app): the first lathe post, fanuc-lathe.cps - diameter programming on the kernel's meridian (M13.5) | 0.8.0 |
| 055d1e0 | 2026-11-06 | feat(app,motion): axial drilling on the lathe, and a rotary letter gate against the machine's inventory (M13.5) | 0.8.0 |
| 259d021 | 2026-11-06 | perf(app,sim,ui): a playback jump cuts checkpoint to checkpoint with the ledger off; the CPU fallback eye traces on a worker (M13.1) | 0.8.0 |
| eaf88bc | 2026-11-06 | feat(app,ui): boundaries read in the tilted plane, and the boundary group on the whole 3D finish family (M11.5) | 0.8.0 |
| 1bad4af | 2026-11-06 | feat(motion,sim): the swarf pose laws - normal retract, a solid reading for leaning rapids, no upright pose under the equator; the mid-move pose law lands (M13.7) | 0.8.0 |
| ee2ebef | 2026-11-06 | feat(ui,app): the machined mesh reaches the viewport's stock slot and the pick (M13.1) | 0.8.0 |
| 191b9aa | 2026-11-06 | feat(motion): the waterline ladder reads the part's slope, with the 45-degree law as its ceiling (M13.9) | 0.8.0 |
| 485e5f2 | 2026-11-06 | feat(app): the setup sheet names each fixture's workholding frame; setup_sheet.rs split along its seams (M13.8) | 0.8.0 |
| 33242f3 | 2026-11-06 | feat(sim): the lathe sweep and the rotary chord pre-pass on the field stock (M13.1) | 0.8.0 |
| fefe8ae | 2026-11-06 | feat(sim,app,ui): the scallop law lives as a non-blocking, ratcheted verdict (M13.9) | 0.8.0 |
| 2e678dd | 2026-11-06 | fix(motion): HSM growth revolutions land on their ring tangentially; a plunge landing clears the full rim (M11.4) | 0.8.0 |
| df18fb7 | 2026-11-06 | refactor(app): operation/material.rs split along its seams - a pure move, 749 -> 216 + 262 + 300 (400-line law) | 0.8.0 |
| 5927926 | 2026-11-06 | fix(motion): one descent per level on the 3D ladder and the bore contour (M11.3 owed) | 0.8.0 |
| c5e002a | 2026-11-06 | fix(ui): themes are found beside the exe at runtime, not through the build machine's source tree (M13.3, 0.8.0 must) | 0.8.0 |
| 9b51275 | 2026-11-06 | feat(app,motion,ui): the open Contour's swept band in the viewport, approach extensions on open runs, and a summary that names the chain's ends (M11.5) | 0.8.0 |
| 76f8d27 | 2026-11-06 | fix(app,ui): the generate-time fixture check runs the workholding laws with the spindle, and fixtures get a Fixed toggle (M13.8) | 0.8.0 |
