//! Транслитерация RU↔EN (аналог `translit-ru.dat` / `translit-en.dat`).
//!
//! В Punto это таблица имён собственных (`Август=August`, `Abraham=Абрахам`).
//! Своих файлов Яндекса мы не поставляем: здесь простой табличный транслит
//! плюс возможность подгрузить собственную таблицу `левое<TAB>правое`.

use std::collections::HashMap;

#[derive(Default, Clone)]
pub struct Translit {
    ru_to_en: HashMap<String, String>,
    en_to_ru: HashMap<String, String>,
}

const PAIRS: &[(&str, &str)] = &[
    ("а", "a"), ("б", "b"), ("в", "v"), ("г", "g"), ("д", "d"),
    ("е", "e"), ("ё", "e"), ("ж", "zh"), ("з", "z"), ("и", "i"),
    ("й", "y"), ("к", "k"), ("л", "l"), ("м", "m"), ("н", "n"),
    ("о", "o"), ("п", "p"), ("р", "r"), ("с", "s"), ("т", "t"),
    ("у", "u"), ("ф", "f"), ("х", "kh"), ("ц", "ts"), ("ч", "ch"),
    ("ш", "sh"), ("щ", "shch"), ("ъ", ""), ("ы", "y"), ("ь", ""),
    ("э", "e"), ("ю", "yu"), ("я", "ya"),
];

impl Translit {
    pub fn new() -> Translit {
        let mut t = Translit::default();
        for (ru, en) in PAIRS {
            t.ru_to_en.insert(ru.to_string(), en.to_string());
            t.en_to_ru.entry(en.to_string()).or_insert_with(|| ru.to_string());
        }
        t
    }

    /// Построчный формат `левое<TAB>правое` (регистр сохраняется).
    pub fn load(path: &std::path::Path) -> Translit {
        let mut t = Translit::new();
        if let Ok(text) = std::fs::read_to_string(path) {
            for line in text.lines() {
                let line = line.trim_end_matches(['\r', '\n']);
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((a, b)) = line.split_once('\t') {
                    t.ru_to_en.insert(a.trim().to_string(), b.trim().to_string());
                    t.en_to_ru.insert(b.trim().to_lowercase(), a.trim().to_string());
                }
            }
        }
        t
    }

    /// RU → Latin посимвольно (первая буква заглавная сохраняется).
    pub fn ru_to_latin(&self, s: &str) -> String {
        let mut out = String::new();
        let mut upper_next = false;
        for ch in s.chars() {
            if ch == ' ' {
                out.push(' ');
                upper_next = true;
                continue;
            }
            let key = ch.to_lowercase().to_string();
            let mapped = self.ru_to_en.get(&key).cloned().unwrap_or_else(|| key.clone());
            if ch.is_uppercase() || upper_next {
                let mut it = mapped.chars();
                if let Some(first) = it.next() {
                    out.extend(first.to_uppercase());
                    out.push_str(it.as_str());
                }
                upper_next = false;
            } else {
                out.push_str(&mapped);
            }
        }
        out
    }

    /// Latin → RU посимвольно.
    pub fn latin_to_ru(&self, s: &str) -> String {
        let mut out = String::new();
        for ch in s.chars() {
            let key = ch.to_lowercase().to_string();
            let mapped = self.en_to_ru.get(&key).cloned().unwrap_or_else(|| key.clone());
            if ch.is_uppercase() {
                let mut it = mapped.chars();
                if let Some(first) = it.next() {
                    out.extend(first.to_uppercase());
                    out.push_str(it.as_str());
                }
            } else {
                out.push_str(&mapped);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translit_basic() {
        let t = Translit::new();
        assert_eq!(t.ru_to_latin("Привет"), "Privet");
        assert_eq!(t.latin_to_ru("Privet"), "Привет");
    }
}
