# 452 - License change: MIT to Apache-2.0, held by the contributors

**Chosen:** CamCraft's own code is licensed under the Apache License 2.0.
The copyright belongs to "The CamCraft contributors" (in `NOTICE`), each
contributor holding the rights to their own work, not to the maintainer
personally. This is the maintainer's choice (#452).

- `LICENSE` is the verbatim Apache-2.0 text from
  https://www.apache.org/licenses/LICENSE-2.0.txt (SHA-256
  `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`).
  `aur/LICENSE` is the same file.
- `NOTICE` carries the copyright line, as Apache-2.0 §4(d) expects of
  redistributors. The deb/rpm, AUR, FreeBSD, macOS and Windows release
  packages ship it next to `LICENSE`.
- Inbound = outbound: a pull request is licensed under Apache-2.0
  (license §5). No CLA.

**Why it is allowed:** the previous MIT license (copyright "Camcraft
contributors") permits sublicensing and redistribution, so existing
contributions can be distributed under Apache-2.0. The MIT terms they were
contributed under stay satisfied by keeping this record:

> MIT License - Copyright (c) 2026 Camcraft contributors. Permission is
> hereby granted, free of charge, to any person obtaining a copy of this
> software and associated documentation files (the "Software"), to deal in
> the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions: The above
> copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software. THE SOFTWARE IS PROVIDED
> "AS IS", WITHOUT WARRANTY OF ANY KIND.

**Considered and dropped:**
- Dual MIT OR Apache-2.0, the Rust ecosystem default: not what the
  maintainer asked for.
- A CLA assigning copyright to the maintainer: the opposite of the goal,
  which is that the contributors hold it.

**Unchanged:** third-party notices (`assets/ui/LICENSES.md`, clipper2-rust's
BSL text) and the per-dependency licenses in Cargo.
