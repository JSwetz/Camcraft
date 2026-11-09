## #304 - a deburr leaves each edge run straight up before travelling

- **Chosen:** Before the traverse to each next run, the deburr path
  retracts vertically from where the last run ended to the clearance
  plane (`finishing3d::deburr::retract`), the same move it already made
  once after the last run. Nothing is added when the path already stands
  at clearance (its start).
- **Why:** Each run opened with a rapid to `(next start, clearance)`
  straight from the previous run's end: tip `drop` below the edge, cone
  still touching the break. That rapid ran diagonally up and across the
  part. On a finished part the generator's verify convicted every link as
  `RapidInStock` (the brick: moves 3, 8, 13), and the program sim's
  deepest gouge columns sat along them.
- **Considered and dropped:**
  - A feed retract instead of a rapid: going straight up, the cone's flank
    leaves the 45-degree break at once (the break lies below the flank),
    so nothing is cut on the way out. The path's own final retract has
    always been a rapid, and the verifier passes it.
  - Retracting only to a retract plane: deburr has no retract parameter,
    only the clearance plane its runs already travel at.
- **Cost to change later:** Small. One helper and two calls.
