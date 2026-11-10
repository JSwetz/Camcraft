## #261 - Show where a refused generation was refused (part 2)

**Decided.** Part 2 takes the issue triage's middle ground: a refused
generation does not draw a partial path; it draws the *place* the
refusal names. A generator refusal can now carry one spot -
`GenError::At { msg, at }`, built with `GenError::invalid_at` - and the
spot travels to the UI:

- `operation::run_job` carries it from the generator's frame to world
  through the op's `TiltedFrame` (`to_world`), the same frame the path
  would have taken; with no frame it is already world.
- The generate command turns it into `CommandError::MachiningAt { msg,
  at }`; `CommandError::location()` reads it.
- The UI keeps it in `State::feedback_at`, beside the refusal sentence,
  and only shows it through `State::refusal_focus()`, which answers
  `None` the moment the refusal no longer describes the page (next
  command, undo/redo, another op on the page, a draft edit). The
  viewport draws a 3-axis cross there in the colour-vision palette's
  gouge colour (`path_lines::refusal_mark`), and the op page's refusal
  line becomes a link that frames the spot (`actions::frame_at`,
  orientation and zoom kept - part 1's jump).

First adopters are the refusals that already named a point in their
sentence: Face's narrow strip between two holes (the #260 shape - the
spot is the strip's midpoint on the quoted row, at the cut height),
the drill cycle's R-plane, counterbore, bore-fit and centre-cutting
doors (the hole's mouth), Bore's counterbore and no-circle doors and
Thread's counterbore and no-air doors (the mouth), and the plunge law's
"would plunge axially into solid" (the plunge's bottom). The radial
(cross-hole) drill runs in a spindle frame shifted in XY; its
`in_world` rewrite moves the spot by the same world - local shift it
already applied to the sentence.

**Why.** A refused generation returns `Err` and caches nothing
(`OpStatus` has no "refused" state, generators stop at the first door),
so there is no partial path to show without a per-generator contract
and a new rule that such a path can never post - both of which the
triage called a design call for the owner. The spot is cheap, needs no
owner decision, and can never post: nothing is generated or cached.
About 30 of the ~650 refusal sites already quote a coordinate; the user
was left to read it off the sentence.

**Details chosen.**
- `At` displays exactly like `Invalid` ("invalid machining parameters:
  ..."), and `MachiningAt` exactly like `Machining`: no sentence, log
  line, golden or MCP error text changes when a site starts naming its
  spot.
- A non-finite spot is dropped, never stored (`invalid_at` and
  `map_location` both fall back to `Invalid`), so a bad coordinate can
  never move the camera to NaN or draw at the origin.
- The refusal cross is twice the size of a warned move's cross (3 vs
  1.5 chevron arms), in the same gouge colour, so "refused here" does
  not read as "this move is warned".
- Click-to-frame is view state (`State::cam.target`), never a command,
  like every camera move.
- The spot is in world coordinates by contract at the command boundary;
  inside `run_job_arms` a located refusal must be in the generator's
  frame (the one `m_gen` is in), because `run_job` maps every one.
- `GenError` moved to its own module (`machining2d/gen_error.rs`) so the
  new variant and helpers do not grow `face.rs`; `face.rs` re-exports
  it, so every `super::face::GenError` path is unchanged.
- `RegenerateAll`'s skipped list keeps text only: a batch has no single
  page to mark on.

**Considered and dropped.**
- Drawing the path built so far: needs every generator to return a
  partial path plus the refusal, and an unpostable path state - the
  triage's "large, and a design call". Left to a research issue if the
  owner wants it.
- Parsing coordinates out of the refusal text: fragile, frame-blind (a
  3+2 tilt's local numbers are not world), and silent when a sentence
  changes.
- A location field on every `GenError` variant: touches all ~650
  construction sites for no gain; one opt-in variant keeps the change
  local.
- Showing the spot from the browser row: the op page is where the
  refusal sentence is.

**Cost to change later.** Small. More sites adopt by swapping
`GenError::Invalid(format!(..))` for `GenError::invalid_at(format!(..),
spot)`; a code that matches `GenError::Invalid(_)` on an adopted site
must also accept `At` (one test did). The mark's look is one function
(`path_lines::refusal_mark`).
