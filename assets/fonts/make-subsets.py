#!/usr/bin/env python3
"""Builds the Noto Sans subsets the launcher bundles for Japanese, Chinese,
and Hindi.

Noto Sans is the same humanist design family as Open Sans, so the three sit
well beside each other. The full CJK fonts are 10-18 MB each, so each is cut
down to what the interface needs:

  * every character used by that language's translation files,
  * kana, punctuation, and full-width forms,
  * the commonly used kanji (JIS X 0208 level 1) or hanzi (GB 2312 level 1),

which covers ordinary app names too. Anything rarer falls back to a font the
system has. Run this again whenever a translation adds a character; a test
fails if one is missing.

    python3 -m venv venv && venv/bin/pip install fonttools brotli
    venv/bin/python assets/fonts/make-subsets.py <folder with the variable fonts>

The variable fonts come from https://github.com/google/fonts/tree/main/ofl
(notosansjp, notosanssc, notosansdevanagari) and are licensed under the SIL
Open Font License (see NotoSans-OFL.txt).
"""

import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

here = Path(__file__).resolve().parent
locales = here.parent.parent / "apps" / "remade-launcher" / "locales"
source = Path(sys.argv[1])


def used_characters(folder: str) -> set[str]:
    text = "".join(path.read_text(encoding="utf-8") for path in (locales / folder).glob("*.ftl"))
    return {c for c in text if ord(c) > 0x7F}


def level_one(codec: str, first: int, last: int) -> set[str]:
    """Characters of a two-byte legacy set, rows `first` to `last`."""
    found = set()
    for row in range(first, last + 1):
        for cell in range(0xA1, 0xFF):
            try:
                found.add(bytes([row, cell]).decode(codec))
            except UnicodeDecodeError:
                pass
    return found


def kana_and_punctuation() -> set[str]:
    ranges = [
        (0x3000, 0x303F),  # CJK punctuation
        (0x3040, 0x30FF),  # hiragana and katakana
        (0xFF00, 0xFFEF),  # full-width forms
        (0x2010, 0x2027),  # dashes, quotes, bullets, ellipsis
        (0x00A0, 0x00FF),  # Latin-1
    ]
    return {chr(code) for low, high in ranges for code in range(low, high + 1)}


def build(name: str, variable: str, weights: dict[str, int], characters: set[str] | None):
    for label, weight in weights.items():
        font = TTFont(source / variable)
        axes = {"wght": weight}
        if "wdth" in {axis.axisTag for axis in font["fvar"].axes}:
            axes["wdth"] = 100
        font = instancer.instantiateVariableFont(font, axes)

        options = subset.Options()
        options.layout_features = ["*"]
        options.name_IDs = [1, 2, 3, 4, 6]
        options.notdef_outline = True
        options.hinting = False
        options.desubroutinize = True
        subsetter = subset.Subsetter(options)

        if characters is None:
            subsetter.populate(unicodes=[cp for cp in font.getBestCmap()])
        else:
            subsetter.populate(text="".join(sorted(characters)))

        subsetter.subset(font)
        out = here / f"{name}-{label}.ttf"
        font.save(out)
        print(out.name, out.stat().st_size // 1024, "KiB")


japanese = used_characters("ja") | kana_and_punctuation() | level_one("euc_jp", 0xB0, 0xCF)
chinese = used_characters("zh-CN") | kana_and_punctuation() | level_one("gb2312", 0xB0, 0xD7)

weights = {"Regular": 400, "SemiBold": 600}

build("NotoSansJP-Subset", "NotoSansJP.var.ttf", weights, japanese)
build("NotoSansSC-Subset", "NotoSansSC.var.ttf", weights, chinese)
build("NotoSansDevanagari", "NotoSansDevanagari.var.ttf", weights, None)
