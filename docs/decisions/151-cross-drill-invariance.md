<!-- moved from RECOMMENDED.md for PR #243 (docs/decisions rule, PR #250) -->

## #151 - cross-drill topology and portable chain regression

- **Chosen:** coalesce rapid-chain points only when their Euclidean separation is at most `1e-9 mm`, absorbing floating-point roundoff locally. Keep routing through `PathBuilder.push` so stationary pose changes remain represented; leave the global CL builder exact-equality semantics unchanged.
- **Regression:** force near-equal radius values and vary G54 offsets. Compare ordered Move kinds, feeds, and poses exactly, and endpoints within `1e-9`, instead of relying on opaque raw-float FNV fingerprints.
- **Safety boundary:** retain the existing stock, part, and holder safety proofs; do not round stock radius inward.
- **Dropped:** a global `PathBuilder` tolerance (too broad), raw-bit fingerprints (platform-dependent), rounding stock radius inward (unnecessary safety impact), and loosening move-count assertions (hides a topology defect).
- **Cost to change later:** revise the local rapid helper and the focused motion/app regressions; no shared CL API, file format, dependency, or saved-job migration is required.
