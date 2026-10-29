# Third-party notices: bundled fonts

Every bundled font is licensed under the **SIL Open Font License, Version 1.1**
(OFL-1.1) and nothing else. A test (`crates/meari/tests/font_catalog.rs`,
`every_bundled_font_is_ofl_and_catalogued`) fails if any file under
`crates/meari/assets/fonts` is not a catalogued face or an OFL-1.1 notice.
All files are unchanged upstream release files (no subsetting, no conversion), except
the **Meari Serif KR** Web slices: unicode-range WOFF2 subsets of Source Han Serif KR
Regular, an OFL-1.1 *Modified Version*. Because Adobe reserves the font name ‘Source’,
those slices are renamed (name table and CFF) to the non-reserved family "Meari Serif KR";
they keep Adobe's copyright notice and the unchanged OFL text. Reproduction:
`scripts/fonts/slice-source-han-serif-kr.py`.

| Family | Files (crates/meari/assets/fonts/…) | Copyright holder | Reserved Font Name | Upstream | Full notice |
|---|---|---|---|---|---|
| Pretendard Variable | pretendard/PretendardVariable.ttf, .woff2 | Copyright (c) 2021, Kil Hyung-jin | Pretendard | github.com/orioncactus/pretendard release v1.3.9 (Pretendard-1.3.9.zip) | pretendard/LICENSE.txt |
| Pretendard JP (Regular, Bold) | pretendard/PretendardJP-Regular.ttf, .woff2; PretendardJP-Bold.ttf, .woff2 | Copyright (c) 2021, Kil Hyung-jin | Pretendard JP | github.com/orioncactus/pretendard release v1.3.9 (PretendardJP-1.3.9.zip) | pretendard/LICENSE-JP.txt |
| Source Serif 4 Variable (Roman) | source-serif-4/SourceSerif4Variable-Roman.ttf, .woff2 | Copyright 2014 - 2023 Adobe (http://www.adobe.com/) | ‘Source’ (Source is a trademark of Adobe) | github.com/adobe-fonts/source-serif release 4.005R | source-serif-4/LICENSE.md |
| Source Han Serif KR (Regular) | source-han-serif-kr/SourceHanSerifKR-Regular.otf (native) | © 2017-2024 Adobe (http://www.adobe.com/) | ‘Source’ | github.com/adobe-fonts/source-han-serif release 2.003R (13_SourceHanSerifKR.zip) | source-han-serif-kr/LICENSE.txt |
| Meari Serif KR (modified: subset slices of Source Han Serif KR Regular) | meari-serif-kr/MeariSerifKR-Regular.*.woff2 (Web, 11 slices) | © 2017-2024 Adobe (http://www.adobe.com/) | ‘Source’ (not used by the renamed slices) | derived from the file above by scripts/fonts/slice-source-han-serif-kr.py | meari-serif-kr/LICENSE.txt |

Not bundled: Noto Serif KR (OFL-1.1, Google) and the per-OS host fonts named in the
stacks (Apple SD Gothic Neo, Malgun Gothic, Batang, Hiragino, Yu Gothic/Mincho, Segoe UI);
they are used only when installed on the host. Arimo, Tinos
and Liberation are never bundled; they are host fonts used only by the Chrome
pixel-comparison path (see `.spec/sot/fonts.sot.md`).

SIL OPEN FONT LICENSE Version 1.1 — full text in each notice file above.
