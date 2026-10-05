# Shared bundled font resource

`NotoSans-Regular.ttf` is unmodified Noto Sans Regular from
<https://github.com/notofonts/noto-fonts/blob/ffebf8c1ee449e544955a7e813c54f9b73848eac/hinted/ttf/NotoSans/NotoSans-Regular.ttf>.

SHA-256: `b85c38ecea8a7cfb39c24e395a4007474fa5a4fc864f6ee33309eb4948d232d5`.

Copyright 2018 The Noto Project Authors. Distributed under SIL Open Font License
1.1; the complete notice is in `OFL.txt`. The font is shared by application UI
typography and authored Motion text. The notice is embedded in the motion library for
presentation in the Text inspector. Include that notice in binary distributions.

`FontSource::NotoSansRegularV1` refers to these exact bytes, not whichever system
font happens to have this name. Missing glyphs are errors, not silent fallback.
Changing these bytes requires a new resource identity and evaluation build ID.
