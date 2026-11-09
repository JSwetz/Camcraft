## #229 - a turning pass that ends inside a longer bar leaves its cut at feed

- **Chosen:** `StockEnv` carries the bar's far end (`s_top`, the highest
  station the stock reaches along the part's axis, read in the part's
  frame). When the bar runs past a profile ride's top station by more
  than float noise (`StockEnv::runs_past`), the ride leaves its cut with
  a feed move at the plunge feed out to the clear radius
  (`prof::Exit::Feed`) instead of a rapid. Profile rough and profile
  finish both read it; they are the two kinds that ride this way and
  read the stock.
- **Why:** Each pass ends at the window's top. If the bar goes on past
  it, the nose sits inside the bar's uncut end, and the radial move out
  crosses stock. As a rapid, the meridian law (#103) rightly flagged it
  (`RapidInStock`, one per rung) and blocked the post, so a normal job
  with a bar longer than the part could not be roughed and posted. The
  same holds when the window stops partway along the bar. A feed move
  is what the issue asked for, and it is a cut: the stock sim and the
  gouge laws judge it as one.
- **Rapid kept where nothing stands:** A bar that ends with the part (or
  no readable stock) keeps the rapid exit, so those paths are byte for
  byte what they were. `LONGER = 1e-6` mirrors `WIDER`: a bar the part's
  own length reads the part's end to the float.
- **Considered and dropped:** Always leaving at feed whenever stock is
  readable. Simpler, but it changes every existing lathe path and slows
  bars that end with the part, for no safety gain. Retracting back along
  the cut was also dropped: longer, and it re-rubs the finished wall.
  Splitting the exit into a feed to the bar's OD and a rapid for the last
  clearance millimetre was dropped too: it saves under a second per pass
  and adds a move to every rung.
- **Not changed:** The contour kinds (chamfer, taper, multi-pass) do not
  read the stock today and keep the rapid exit. Facing already reads the
  bar's end (#122).
- **Cost to change later:** Small. The rule is one method and one enum;
  a kind that should exit another way adds an `Exit` arm.
