# Fonts

`NotoSans-Light.ttf`, `NotoSans-Regular.ttf`, `NotoSans-Medium.ttf` and `NotoSans-Bold.ttf` (the Latin, Greek and Cyrillic build of Noto Sans, 2022) are embedded
into `quench-text` so that text looks the same on every machine.

Their licence is the **SIL Open Font License, Version 1.1** (`OFL.txt`): the fonts' own `name` table says so (name ID
13: "This Font Software is licensed under the SIL Open Font License, Version 1.1"; ID 14: https://scripts.sil.org/OFL),
and `quench-text`'s tests read those strings and fail if they change. Copyright 2022 The Noto Project Authors
(https://github.com/notofonts/latin-greek-cyrillic). The fonts are used unmodified; the licence and this notice go
wherever the fonts go (a published `quench-text` package carries this directory).

`NotoSansMath-Regular.ttf` (Noto Sans Math, version 3.000, TrueType `glyf` outlines with an OpenType `MATH` table) is
embedded only with the `math-font` feature (`noto_sans_math()`), for typesetting formulas. It was downloaded on
2026-11-08 from Google Fonts, https://github.com/google/fonts/raw/main/ofl/notosansmath/NotoSansMath-Regular.ttf
(1 015 396 bytes, SHA-256 `3f495fe933c06786e4d5f6d86b8ee70b6753a68ee3b9d87528726de0f6e2c47d`), and is used
unmodified. Its licence is the same **SIL Open Font License, Version 1.1** (`OFL.txt`; name ID 13 of the font says so,
ID 14: https://openfontlicense.org); its `name` table gives the copyright as "Copyright 2022 Google LLC. All Rights
Reserved." (the Noto Project). The tests read these strings.
