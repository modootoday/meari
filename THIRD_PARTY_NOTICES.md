# Third-party notices: fonts

No font file is committed to this repository and no font byte is compiled into any binary.

The canonical fonts are downloaded on request (`meari fonts fetch`, `cargo xtask fonts-fetch`)
from github.com/google/fonts at revision 6157dfaa640ca5738b4d1a7070fe389c3036d7fc into a
local cache (`runtime/fonts/`, git-ignored). Each file's URL, length and sha256, and its
directory's `OFL.txt` (saved unchanged as `<dir>-OFL.txt`), are pinned in
`crates/meari/src/domain/design/fonts.rs`.

| Family | Licence | Copyright | Reserved Font Name |
|---|---|---|---|
| Noto Sans, Noto Sans Italic, Noto Serif, Noto Serif Italic, Noto Sans Mono | SIL OFL-1.1 | The Noto Project Authors (github.com/notofonts) | none |
| Noto Sans KR, Noto Serif KR, Noto Sans JP, Noto Serif JP | SIL OFL-1.1 | The Noto Project Authors; Adobe (Source Han) | 'Source' (not used by any name here) |
| Noto Sans Arabic, Noto Naskh Arabic, Noto Sans/Serif Hebrew, Noto Sans/Serif Devanagari, Noto Sans/Serif Thai | SIL OFL-1.1 | The Noto Project Authors | none |

Web WOFF2 unicode-range subsets are generated locally from the cached files (an OFL
*Modified Version*; family names unchanged, none contains a reserved name). Font bytes are not
relicensed under this project's dual license. Host fonts named as fallbacks (Apple SD Gothic
Neo, Malgun Gothic, Segoe UI, Arial, …) are never bundled.

SIL OPEN FONT LICENSE Version 1.1 — full text in each cached `<dir>-OFL.txt`.
