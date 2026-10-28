# Fonts

`NotoSans-Regular.ttf` and `NotoSans-Bold.ttf` (the Latin, Greek and Cyrillic build of Noto Sans, 2022) are embedded
into `quench-text` so that text looks the same on every machine.

Their licence is the **SIL Open Font License, Version 1.1** (`OFL.txt`): the fonts' own `name` table says so (name ID
13: "This Font Software is licensed under the SIL Open Font License, Version 1.1"; ID 14: https://scripts.sil.org/OFL),
and `quench-text`'s tests read those strings and fail if they change. Copyright 2022 The Noto Project Authors
(https://github.com/notofonts/latin-greek-cyrillic). The fonts are used unmodified; the licence and this notice go
wherever the fonts go (a published `quench-text` package carries this directory).
