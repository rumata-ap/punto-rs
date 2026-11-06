//! Словарь и функция сопоставления — точный перенос `sub_41F9C0` из Punto 4.5.
//!
//! Формат файла (свой, не яндексовый): по строке на запись
//!
//! ```text
//! # комментарий
//! <ПРИЗНАКИ><TAB><текст>
//! ```
//!
//! ПРИЗНАКИ — до шести букв `P B C E A D` (порядок любой, может быть пусто):
//!
//! | буква | бит    | смысл в Punto                                  |
//! |-------|--------|------------------------------------------------|
//! | `P`   | `0x01` | точное совпадение                              |
//! | `B`   | `0x08` | совпадение по префиксу                         |
//! | (нет) | `0x02` | вхождение подстроки                            |
//! | `C`   | `0x04` | сравнивать с учётом регистра                    |
//! | `E`   | `0x10` | «настоящее слово языка этой раскладки»         |
//! | `A`   | `0x20` | признак A (наружу, в решении не участвует)     |
//! | `D`   | `0x40` | признак D (наружу, вместе с `E` — user-записи) |
//!
//! Бит `0x80` в Punto выставляется всегда; здесь он тоже выставляется,
//! чтобы решение переносилось без изменений.

pub const F_EXACT: u8 = 0x01;
pub const F_SUBSTR: u8 = 0x02;
pub const F_CASE: u8 = 0x04;
pub const F_PREFIX: u8 = 0x08;
pub const F_E: u8 = 0x10;
pub const F_A: u8 = 0x20;
pub const F_D: u8 = 0x40;
pub const F_ALWAYS: u8 = 0x80;

pub fn flags_from_letters(letters: &str) -> u8 {
    let mut f = 0u8;
    let mut has_match = false;
    for ch in letters.chars() {
        match ch.to_ascii_uppercase() {
            'P' => { f |= F_EXACT; has_match = true; }
            'B' => { f |= F_PREFIX; has_match = true; }
            'C' => f |= F_CASE,
            'E' => f |= F_E,
            'A' => f |= F_A,
            'D' => f |= F_D,
            _ => {}
        }
    }
    if !has_match {
        f |= F_SUBSTR;
    }
    f | F_ALWAYS
}

/// Признаки, которые Punto отдаёт вызывающему коду (`out[0..4]`).
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Flags {
    pub e: bool,
    pub a: bool,
    pub d: bool,
    pub always: bool,
}

impl Flags {
    pub fn from_bits(f: u8) -> Flags {
        Flags {
            e: f & F_E != 0,
            a: f & F_A != 0,
            d: f & F_D != 0,
            always: f & F_ALWAYS != 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub text: String,
    pub folded: String,
    pub flags: u8,
}

#[derive(Clone, Default, Debug)]
pub struct Dict {
    pub entries: Vec<Entry>,
    /// символы, из которых состоит «слово» (обрезка хвоста строки)
    pub charset: String,
}

/// Результат сопоставления: совпало ли и признаки совпавшей записи.
#[derive(Clone, Copy, Default, Debug)]
pub struct Match {
    pub hit: bool,
    pub flags: Flags,
}

impl Dict {
    pub fn new(charset: &str) -> Dict {
        Dict { entries: Vec::new(), charset: charset.to_string() }
    }

    pub fn add(&mut self, text: &str, flags: u8) {
        let folded = if flags & F_CASE != 0 {
            text.to_string()
        } else {
            text.to_lowercase()
        };
        self.entries.push(Entry { text: text.to_string(), folded, flags });
    }

    /// Разбор нашего текстового формата.
    pub fn load(text: &str, charset: &str) -> Dict {
        let mut d = Dict::new(charset);
        for line in text.lines() {
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (letters, body) = match line.find('\t') {
                Some(i) => (&line[..i], &line[i + 1..]),
                None => ("", line),
            };
            if body.is_empty() {
                continue;
            }
            d.add(body, flags_from_letters(letters));
        }
        d
    }

    /// Обрезка строки до хвостового фрагмента из символов набора.
    /// Соответствует `sub_41A630` + обрезке в `sub_41F9C0`.
    pub fn trim_word(&self, s: &str) -> String {
        if self.charset.is_empty() {
            return s.to_string();
        }
        let chars: Vec<char> = s.chars().collect();
        let mut start = chars.len();
        while start > 0 && self.charset.contains(chars[start - 1]) {
            start -= 1;
        }
        chars[start..].iter().collect()
    }

    /// Основная функция: `sub_41F9C0(dict, Str, out)`.
    pub fn match_word(&self, s: &str) -> Match {
        let trimmed = self.trim_word(s);
        if trimmed.is_empty() {
            return Match::default();
        }
        let lower = trimmed.to_lowercase();
        let orig = trimmed.as_str();
        let mut last = Flags::default();
        for e in &self.entries {
            let flags = e.flags;
            last = Flags::from_bits(flags);
            let hay = if flags & F_CASE != 0 { orig } else { lower.as_str() };
            let needle = if flags & F_CASE != 0 { e.text.as_str() } else { e.folded.as_str() };
            let hit = if flags & F_PREFIX != 0 {
                hay.starts_with(needle)
            } else if flags & F_EXACT != 0 {
                hay == needle
            } else {
                hay.contains(needle)
            };
            if hit {
                return Match { hit: true, flags: last };
            }
        }
        Match { hit: false, flags: last }
    }
}
