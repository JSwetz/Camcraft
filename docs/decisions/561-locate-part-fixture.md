## #561 - Locate the part in a fixture (or the fixture on the part) by mates (2026-11-09)

- **Chosen: one solve, two commands.** `fixture_locate::plan` reads up to
  three mates (part face/vertex against fixture face/vertex) in the frame
  of the setup the fixture is bound to and returns one rigid transform
  that carries the part onto the fixture. `LocatePart` applies it to the
  part, `LocateFixture` applies its inverse to the fixture (so the
  relative placement, nudge included, is the same whichever body moves).
  `Locate` wraps both for the UI and MCP, which offer the choice.
- **Solver: sequential 3-2-1, faces orient, every mate translates.** The
  first face-to-face mate opposes its normals by the shortest turn; the
  second turns about the first fixture normal (refused by name when its
  angle to the first face does not match); a third must already agree.
  Then each mate pins the translation along the directions still free,
  in mate order (Gram-Schmidt over the pinned directions); a mate the
  earlier ones contradict is refused with the miss in mm. Supported
  pairs: face-face, point-point, point-on-face and face-on-point. With
  fewer than three mates the free freedoms are left where they were
  (the minimal move), which is what "partial location" means on the
  bench. Tolerances: 1e-4 (sine) for normals, 1e-4 mm for residuals.
- **The nudge** is a turn about the first face mate's fixture normal
  (else +Z) through its fixture datum, then a shift along the setup's
  X / Y / Z - applied after the mates, part relative to fixture.
- **Which part write.** In a setup that holds the part as modelled the
  locate is a `MoveModel` (document placement; the stock follows the
  part as it always has). In a setup that holds the part turned (#326)
  it composes into that setup's alignment through `AlignPartToSetup`
  (its stock and linked setups follow, the other setups keep the part
  where they hold it). Considered and dropped: always writing a setup
  alignment - a translation-only alignment would make the setup
  "aligned" and `generation_problem` would then refuse every op kind but
  the four 2D ones, and a document with no setups has nowhere to store
  it. Cost of the choice: in an unaligned multi-setup document a locate
  moves the part in every setup that reads the document placement; the
  fix later is per-setup part placement (a follow-up, not filed here).
- **A located part's square stock stays square (review fix).** A box or
  cylinder stock whose axes lie along the setup's (any quarter turn) is
  not turned with the part by a locate: it keeps its own orientation and
  is re-fitted round the part's new place with the allowance it had on
  each side, measured in the stock's own frame before the move (top
  stays top, the -X side stays the -X side; a cylinder keeps its radial
  and end allowance and its axis offset from the part's footprint).
  Why: the vise clamps the blank square - a box stock set square round
  a turned part ended tilted and 17 mm under the vise floor, and a Face
  op faced air. A pure translation moves the block by exactly the part's
  shift (same size), so a locate with no turn reads as before. Considered
  and dropped: translate-only keeping the old size (a quarter turn leaves
  the block the wrong shape round the part); carrying each allowance
  round with the part's faces (a 30-degree turn cannot carry them, and a
  "+2 on top for facing" allowance would end up on a side). A stock that
  is off the axes (it was turned with the part before) or copied from
  the model still follows rigidly. A refit that cannot keep its
  allowance (a stock that did not wrap the part) is refused by name.
  Lives in `crate::stock_follow`; the locate writes through
  `MoveModel::locating` / `AlignPartToSetup::locating` (main setup only;
  linked setups #316 follow as before).
- **Plain `MoveModel` keeps its rigid follow.** A part turned on the
  Model page still carries its stock with it (pinned by
  `commands_undo::moving_one_part_carries_existing_stock_and_undo_restores_both`).
  Considered: the same square rule for every move - consistent, but it
  changes a shipped behaviour outside this issue and its pinned tests;
  worth its own issue (a Model-page turn of a square stock by a non-quarter
  angle still tilts the block).
- **The stock is judged against the fixture too.** The interpenetration
  warning now also checks the setup's stock where the locate leaves it
  (or, for `LocateFixture`, against the moved fixture) and names it: "the
  stock '<name>' would sink into the fixture '<vise>' (near ...)". A
  warning, not a refusal, for the same reason as the part's.
- **Staleness follows the existing laws.** A moved part changes the
  stamps' inputs, so the setup's ops read Stale and undo returns them to
  Ok (pinned). A moved fixture does NOT stale a toolpath: the
  "workholding never rides the stamp" law (`crate::fixture`,
  `workholding_never_stales_a_fresh_toolpath`) holds, and the live
  collision gate re-judges against the new placement (pinned through
  `posting::fixture_findings`). The issue text asked for staleness on a
  fixture move too; breaking the live-gate law would re-generate
  programs whose cut did not change, so the gate's re-judge is the
  honest answer.
- **Interpenetration warns, it does not refuse.** A sampled check (each
  facet centre pushed 0.05 mm into its own body, tested inside the
  other's closed mesh with the fixture mesh's ray-crossing law, at most
  20k samples per body) runs once at construction; the warning names
  the part, the fixture and a point. Refusing was dropped: modelled
  vises are often drawn closed on a nominal part, and the collision gate
  still judges every toolpath against the steel. An open mesh skips the
  check (logged), never guesses.
- **Refusals by name at creation:** no/blank fixture, the machine table,
  a model that is not a fixture, no/blank part, a pick on the wrong
  body, an edge or a curved face, a face with no area or no plane, two
  mates on one point or face (coincident), faces that cannot oppose, a
  mate the earlier ones contradict, no mates, more than three, a
  non-finite nudge, and "nothing would move".
- **UI:** one draft (`crate::locate_pick`, view state) fed by the
  existing Face and Vertex picks - part pick first, then the fixture
  pick closes the mate. Drawn on each Fixtures page row ("Locate
  part…") and on the Model page (a part: "Locate in <fixture>…"; a
  fixture: "Locate part…"); the form lists the mates, offers Move the
  part / Move the fixture, Offset X/Y/Z, Turn, Apply locate (greyed with
  the solve's refusal) and Remove last.
- **UI review fixes (hands-on drive).** Mates read in plain words in
  the fixture's setup frame - a face by its outward normal ("part face -Z
  (bottom), #1"; off the axes, its components) plus its number on the
  body, a point by its coordinates - instead of the raw feature id
  (dropped: naming faces by their role, floor/jaw/stop, which the app
  cannot know). The drafted picks were already outlined in the viewport
  through `selection_feature_lines` (pinned now). The Next hint offers
  Apply only when the solve accepts the draft, else it says why; the
  refusal also stands on its own line under Apply, zero mates included
  (it was hover-only). The offset names its frame (setup X / Y / Z, part
  relative to the fixture) and the turn names its mate and sense (the
  first face-to-face mate's fixture normal, + counter-clockwise looking
  down it; else setup +Z), on wrapped lines above short row labels the
  label column cannot truncate. A fixture pick while a part face is
  wanted adds "hide the fixture (eye) to pick part faces hidden in it".
- **MCP:** `locate_part` (mates by `list_features` ids, offset, turn,
  move), and `list_features` now lists a fixture's features too so its
  face ids can be named.
- **Follow-ups (not in this PR):** point-pair mates do not turn the part
  (two or three vertex pairs cannot orient it - faces orient); a mate
  preview (ghost of the moved body) before Apply; per-setup part
  placement for unaligned multi-setup documents.
