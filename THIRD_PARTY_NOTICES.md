# Third-party notices: bundled fonts

Every bundled font is licensed under the **SIL Open Font License, Version 1.1**
(OFL-1.1) and nothing else. A test (`crates/meari/tests/font_catalog.rs`,
`every_bundled_font_is_ofl_and_catalogued`) fails if any file under
`crates/meari/assets/fonts` is not a catalogued face or an OFL-1.1 notice.
All files are unchanged upstream release files (no subsetting, no conversion).

| Family | Files (crates/meari/assets/fonts/…) | Copyright holder | Reserved Font Name | Upstream | Full notice |
|---|---|---|---|---|---|
| Pretendard Variable | pretendard/PretendardVariable.ttf, .woff2 | Copyright (c) 2021, Kil Hyung-jin | Pretendard | github.com/orioncactus/pretendard release v1.3.9 (Pretendard-1.3.9.zip) | pretendard/LICENSE.txt |
| Pretendard JP (Regular) | pretendard/PretendardJP-Regular.ttf, .woff2 | Copyright (c) 2021, Kil Hyung-jin | Pretendard JP | github.com/orioncactus/pretendard release v1.3.9 (PretendardJP-1.3.9.zip) | pretendard/LICENSE-JP.txt |
| Source Serif 4 Variable (Roman) | source-serif-4/SourceSerif4Variable-Roman.ttf, .woff2 | Copyright 2014 - 2023 Adobe (http://www.adobe.com/) | ‘Source’ (Source is a trademark of Adobe) | github.com/adobe-fonts/source-serif release 4.005R | source-serif-4/LICENSE.md |

Not bundled: Source Han Serif KR / Noto Serif KR (OFL-1.1, Adobe / Google) are
named in the serif stack and used only when installed on the host. Arimo, Tinos
and Liberation are never bundled; they are host fonts used only by the Chrome
pixel-comparison path (see `.spec/sot/fonts.sot.md`).

SIL OPEN FONT LICENSE Version 1.1 — full text in each notice file above.
