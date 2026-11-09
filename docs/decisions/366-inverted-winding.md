## #366 - Face-normal alignment picks check the body's winding first

**Decided.** Refuse at pick time, by name; never repair at import. Every
face-normal read for alignment (`pick::align::outward_normal`, shared by
the single-face plane lay, the two-edge plane lay's shared face and the
3-2-1's Z face) first asks `pick::winding` for the verdict of the body
the face's facets belong to. A body is a connected set of facets (linked
over shared welded edges). Only a body that is closed (every edge used by
exactly two facets, once in each direction) and has a positive signed
volume (divergence theorem, signed tetrahedra to a reference corner) is
`Outward`; its winding is trusted as before. Anything else refuses with
the face and part named and the reason: inside-out (negative volume),
open (an edge on one facet), non-manifold (an edge on three or more),
wound both ways (two facets on an edge in the same direction), or no
volume. The refusal flows through the existing channels: the Apply
button of the plane lay and the "Align WCS (3-2-1)" button grey out with
the reason as their tooltip; a queued command is refused through the
Setup error scope. No UI code changed.

- **Why refuse, not repair.** Flipping facets at import silently changes
  the user's geometry, every downstream reading of it (toolpaths of
  existing projects, goldens, the project file's stored facets), and the
  undo history has no command for it. A refusal changes nothing that
  already works on a correctly wound model; it only stops the gouge-class
  mis-orientation. Dropped: repair at import with a load note (wider
  blast radius, and a mesh that is open or non-manifold still could not
  be repaired); a "Flip normals" command (worth a separate issue if users
  hit inside-out STLs often - the refusal text says to fix the winding).
- **Weld tolerance 1e-4 mm, spatial hash.** The feature module's 1 nm
  lattice is not enough here: the shipped `sample_ap214.step` has seam
  vertices one lattice step apart on a rounding boundary, which read as
  an open body. Corners within 0.1 um weld (27-cell neighbour search, the
  same scheme as the mesh repair pass). Dropped: the exact lattice (false
  "open" on real STEP seams); a coarser tolerance (would collapse real
  micro-features).
- **Per body, worst verdict over the face's facets.** A part with several
  bodies is judged body by body, so a good body still aligns beside a bad
  one. A face whose facets span bodies takes the worst verdict.
- **Cache.** A thread-local MRU of 8 verdicts, keyed by an ordered,
  winding-sensitive hash of the facet coordinates. The feature digest
  could not be reused: it is order- and winding-free, so an inverted mesh
  and its twin share a key. The cache owns no entity and is bounded, so
  deleting a model leaks nothing; a miss recomputes. Dropped: keying on
  (entity, revision) - entity ids repeat across worlds in one thread.
- **Cost to change later.** Adding import-time repair later is additive:
  a repaired body reads `Outward` and the check passes. Loosening (e.g.
  trusting an open body's local winding) would be one match arm in
  `pick::winding::refusal`.
