//! Таблица «клавиша → символ» для двух раскладок.
//!
//! Punto хранит буфер нажатий (VK + Shift + Caps) и получает из него текст
//! для нужной раскладки. Здесь то же самое, только без Win32: фиксированная
//! таблица для стандартных US-QWERTY и RU-ЙЦУКЕН (как в Windows).

/// Нажатие в том виде, в котором оно попадает в буфер.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyPress {
    pub vk: u16,
    pub shift: bool,
    pub caps: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    En,
    Ru,
}

impl Layout {
    pub fn other(self) -> Layout {
        match self {
            Layout::En => Layout::Ru,
            Layout::Ru => Layout::En,
        }
    }
}

struct Row {
    vk: u16,
    en: (char, char),
    ru: (char, char),
}

const ROWS: &[Row] = &[
    Row { vk: 0x41, en: ('a', 'A'), ru: ('ф', 'Ф') },
    Row { vk: 0x42, en: ('b', 'B'), ru: ('и', 'И') },
    Row { vk: 0x43, en: ('c', 'C'), ru: ('с', 'С') },
    Row { vk: 0x44, en: ('d', 'D'), ru: ('в', 'В') },
    Row { vk: 0x45, en: ('e', 'E'), ru: ('у', 'У') },
    Row { vk: 0x46, en: ('f', 'F'), ru: ('а', 'А') },
    Row { vk: 0x47, en: ('g', 'G'), ru: ('п', 'П') },
    Row { vk: 0x48, en: ('h', 'H'), ru: ('р', 'Р') },
    Row { vk: 0x49, en: ('i', 'I'), ru: ('ш', 'Ш') },
    Row { vk: 0x4A, en: ('j', 'J'), ru: ('о', 'О') },
    Row { vk: 0x4B, en: ('k', 'K'), ru: ('л', 'Л') },
    Row { vk: 0x4C, en: ('l', 'L'), ru: ('д', 'Д') },
    Row { vk: 0x4D, en: ('m', 'M'), ru: ('ь', 'Ь') },
    Row { vk: 0x4E, en: ('n', 'N'), ru: ('т', 'Т') },
    Row { vk: 0x4F, en: ('o', 'O'), ru: ('щ', 'Щ') },
    Row { vk: 0x50, en: ('p', 'P'), ru: ('з', 'З') },
    Row { vk: 0x51, en: ('q', 'Q'), ru: ('й', 'Й') },
    Row { vk: 0x52, en: ('r', 'R'), ru: ('к', 'К') },
    Row { vk: 0x53, en: ('s', 'S'), ru: ('ы', 'Ы') },
    Row { vk: 0x54, en: ('t', 'T'), ru: ('е', 'Е') },
    Row { vk: 0x55, en: ('u', 'U'), ru: ('г', 'Г') },
    Row { vk: 0x56, en: ('v', 'V'), ru: ('м', 'М') },
    Row { vk: 0x57, en: ('w', 'W'), ru: ('ц', 'Ц') },
    Row { vk: 0x58, en: ('x', 'X'), ru: ('ч', 'Ч') },
    Row { vk: 0x59, en: ('y', 'Y'), ru: ('н', 'Н') },
    Row { vk: 0x5A, en: ('z', 'Z'), ru: ('я', 'Я') },
    Row { vk: 0xBA, en: (';', ':'), ru: ('ж', 'Ж') },
    Row { vk: 0xBB, en: ('=', '+'), ru: ('=', '+') },
    Row { vk: 0xBC, en: (',', '<'), ru: ('б', 'Б') },
    Row { vk: 0xBD, en: ('-', '_'), ru: ('-', '_') },
    Row { vk: 0xBE, en: ('.', '>'), ru: ('ю', 'Ю') },
    Row { vk: 0xBF, en: ('/', '?'), ru: ('.', ',') },
    Row { vk: 0xC0, en: ('`', '~'), ru: ('ё', 'Ё') },
    Row { vk: 0xDB, en: ('[', '{'), ru: ('х', 'Х') },
    Row { vk: 0xDC, en: ('\\', '|'), ru: ('э', 'Э') },
    Row { vk: 0xDD, en: (']', '}'), ru: ('ъ', 'Ъ') },
];

/// Источник «клавиша → символ».
///
/// По умолчанию — встроенная таблица US/RU. С фичей `xkb` доступен вариант
/// через libxkbcommon: он читает реальные раскладки системы (включая
/// нестандартные) и потому предпочтителен на Linux.
pub enum Mapper {
    Static,
    #[cfg(all(target_os = "linux", feature = "xkb"))]
    Xkb(crate::keymap_xkb::XkbMapper),
}

impl Default for Mapper {
    fn default() -> Mapper {
        Mapper::Static
    }
}

impl Mapper {
    /// Собрать маппер по имени из конфига (`static` | `xkb`).
    pub fn from_name(name: &str, layouts: &str) -> Mapper {
        #[cfg(all(target_os = "linux", feature = "xkb"))]
        if name.eq_ignore_ascii_case("xkb") {
            if let Some(m) = crate::keymap_xkb::XkbMapper::new(layouts) {
                return Mapper::Xkb(m);
            }
            eprintln!("punto-rs: xkb-маппер не создался, беру встроенную таблицу");
        }
        let _ = (name, layouts);
        Mapper::Static
    }

    pub fn text_for(&self, keys: &[KeyPress], layout: Layout) -> String {
        match self {
            Mapper::Static => text_for(keys, layout),
            #[cfg(all(target_os = "linux", feature = "xkb"))]
            Mapper::Xkb(m) => m.text_for(keys, layout),
        }
    }
}

/// Символ, который даст это нажатие в указанной раскладке.
pub fn key_char(k: KeyPress, layout: Layout) -> Option<char> {
    if k.vk == 0x20 {
        return Some(' ');
    }
    let row = ROWS.iter().find(|r| r.vk == k.vk)?;
    let (lo, hi) = match layout {
        Layout::En => row.en,
        Layout::Ru => row.ru,
    };
    let letter = lo.is_lowercase() || lo.is_uppercase();
    let upper = if letter {
        k.shift ^ k.caps
    } else {
        k.shift
    };
    Some(if upper { hi } else { lo })
}

/// Текст, который получится из буфера в указанной раскладке.
pub fn text_for(keys: &[KeyPress], layout: Layout) -> String {
    keys.iter().filter_map(|k| key_char(*k, layout)).collect()
}

/// Обратный поиск: чем набирается символ (первая подходящая клавиша).
pub fn char_key(c: char) -> Option<KeyPress> {
    if c == ' ' {
        return Some(KeyPress { vk: 0x20, shift: false, caps: false });
    }
    for row in ROWS {
        if row.en.0 == c || row.en.1 == c || row.ru.0 == c || row.ru.1 == c {
            let shift = c == row.en.1 || c == row.ru.1;
            return Some(KeyPress { vk: row.vk, shift, caps: false });
        }
    }
    None
}

/// «Та же последовательность клавиш, но в другой раскладке».
pub fn convert(text: &str, from: Layout, to: Layout) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match char_key(ch) {
            Some(k) => out.push(key_char(k, to).unwrap_or(ch)),
            None => out.push(ch),
        }
    }
    let _ = from;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_between_layouts() {
        assert_eq!(convert("ghbdtn", Layout::En, Layout::Ru), "привет");
        assert_eq!(convert("привет", Layout::Ru, Layout::En), "ghbdtn");
        assert_eq!(convert("Ghbdtn", Layout::En, Layout::Ru), "Привет");
        // цифры и одинаковые знаки не меняются
        assert_eq!(convert("123 = -", Layout::En, Layout::Ru), "123 = -");
    }

    #[test]
    fn caps_and_shift() {
        let k = KeyPress { vk: 0x46, shift: true, caps: false };
        assert_eq!(key_char(k, Layout::Ru), Some('А'));
        let k2 = KeyPress { vk: 0x46, shift: false, caps: true };
        assert_eq!(key_char(k2, Layout::Ru), Some('А'));
        assert_eq!(key_char(k2, Layout::En), Some('F'));
    }
}
