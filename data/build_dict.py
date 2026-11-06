#!/usr/bin/env python3
"""Сборка словарей punto-rs из открытых источников (hunspell / wordlist).

Ничего яндексового не использует: берём свободные словари Debian и сами
генерируем «мусорный» слой — русские слова, набранные в EN-раскладке.

Пример:
    sudo apt install hunspell-ru hunspell-en-us
    python3 data/build_dict.py \
        --ru /usr/share/hunspell/ru_RU.dic \
        --en /usr/share/hunspell/en_US.dic \
        --out ~/.config/punto-rs

Результат: ru.dict, en.dict, wrong.dict в формате `<PBCEAD><TAB>текст`.
"""
import argparse
import pathlib
import re

# позиция клавиши → пары символов (как в src/keymap.rs)
ROWS = [
    ("qwertyuiop", "йцукенгшщз"),
    ("asdfghjkl;'", "фывапролджэ"),
    ("zxcvbnm,./", "ячсмитьбю."),
]

RU_TO_EN = {}
for en_row, ru_row in ROWS:
    for en, ru in zip(en_row, ru_row):
        RU_TO_EN[ru] = en
        RU_TO_EN.setdefault(ru.upper(), en.upper())
RU_TO_EN.update({"ё": "`", "Ё": "~"})

WORD = re.compile(r"^[А-Яа-яЁёA-Za-z-]{2,}$")


def read_hunspell(path: pathlib.Path, max_words: int) -> list[str]:
    """Читает .dic: первая строка — количество, далее «слово/ФЛАГИ»."""
    out = []
    if not path.exists():
        print(f"  ! нет файла {path}")
        return out
    with path.open(encoding="utf-8", errors="replace") as fh:
        next(fh, None)
        for line in fh:
            w = line.split("/")[0].strip()
            if WORD.match(w):
                out.append(w)
            if len(out) >= max_words:
                break
    return out


def to_wrong_layout(words: list[str]) -> list[str]:
    out = []
    for w in words:
        if all(ch in RU_TO_EN or ch == "-" for ch in w):
            out.append("".join(RU_TO_EN.get(ch, ch) for ch in w))
    return out


def write(path: pathlib.Path, flags: str, words: list[str], header: str) -> None:
    seen, lines = set(), [f"# {header}"]
    for w in words:
        key = w.lower()
        if key in seen:
            continue
        seen.add(key)
        lines.append(f"{flags}\t{w}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"  {path.name}: {len(lines) - 1} записей")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ru", type=pathlib.Path, help="ru_RU.dic")
    ap.add_argument("--en", type=pathlib.Path, help="en_US.dic")
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--max", type=int, default=50000, help="лимит слов на язык")
    args = ap.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    ru = read_hunspell(args.ru, args.max) if args.ru else []
    en = read_hunspell(args.en, args.max) if args.en else []

    if ru:
        write(args.out / "ru.dict", "PE", ru, "русские слова: точное + E")
        write(args.out / "wrong.dict", "B", to_wrong_layout(ru),
              "те же слова в EN-раскладке: префикс, без E")
    if en:
        write(args.out / "en.dict", "PE", en, "английские слова: точное + E")

    print("готово:", args.out)


if __name__ == "__main__":
    main()
