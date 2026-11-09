## #506 - Browser tree rows readable on the light panel

**Decided:** two changes.
1. `theme::apply_to` writes the token table into BOTH of egui's theme
   slots (dark and light). Root cause: egui follows the OS appearance, and
   only the slot active at startup got the table, so a light-mode Mac drew
   the tree's ink on egui's stock light panel. The document's theme is
   now the only theme; the OS appearance never swaps it out.
2. Tree row ink (`painted::row_inks`): a label is body text; the suffix
   and hidden rows use the dim tier, never the faint (disabled) tier,
   which read about 3:1. A selected row is all body text, because the dim
   tier only reads 3.5:1 on the dark theme's selection fill.

3. Tree icons and the eye (#524 review by hand): their PNGs are white masks and were tinted with plain white, which washes out on the light panel. A row's icon now takes the row's label ink and its eye the suffix ink, the same inks the contrast test proves, the way the ribbon tints its icons.

**Why:** every row ink must clear WCAG body contrast (4.5:1) on the ground
it is painted on. `tree_ink_tests` pins this for both themes and all four
row grounds.

**Considered and dropped:** darkening `text_faint` across the theme. That
tier marks disabled controls everywhere, and a hidden tree row is not
disabled. Following the OS appearance (picking the dark or light table
from the system): the theme switcher is the user's choice, and two
sources of truth made this bug in the first place.

**Cost to change later:** low. One function and one loop.
