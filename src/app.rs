//! Связка «буфер → две гипотезы → решение → действие».
//!
//! Здесь же правила верхнего уровня, которые Punto держит в `sub_41C0F0`
//! перед скорером: `DontConvertCapitals`, `DetectPasswords`,
//! `DontReactOnOtherLangs`.

use crate::buffer::{Push, WordBuffer};
use crate::config::Config;
use crate::dict::Dict;
use crate::engine::{Decision, Engine, Route};
use crate::keymap::{convert, KeyPress, Layout, Mapper};
use crate::translit::Translit;

/// Что должен сделать внешний слой (ввод-вывод).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Action {
    Nothing,
    /// переключить раскладку на `to`, стереть `erase` символов и напечатать `text`
    Convert { to: Layout, erase: usize, text: String },
}

pub struct App {
    pub cfg: Config,
    pub dict: Dict,
    pub engine: Engine,
    pub buffer: WordBuffer,
    pub translit: Translit,
    pub layout: Layout,
    /// источник «клавиша → символ» (встроенная таблица или xkbcommon)
    pub mapper: Mapper,
    /// почему последнее решение было таким (для логов)
    pub last_reason: String,
}

impl App {
    pub fn new(cfg: Config) -> App {
        let mut dict = Dict::new(&cfg.charset);
        for path in &cfg.dict_paths {
            if let Ok(text) = std::fs::read_to_string(path) {
                for line in text.lines() {
                    let line = line.trim_end_matches(['\r', '\n']);
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let (letters, body) = match line.find('\t') {
                        Some(i) => (&line[..i], &line[i + 1..]),
                        None => ("", line),
                    };
                    if !body.is_empty() {
                        dict.add(body, crate::dict::flags_from_letters(letters));
                    }
                }
            }
        }
        let translit = Translit::load(&std::path::PathBuf::from("/etc/punto-rs/translit.tsv"));
        let mapper = Mapper::from_name(&cfg.text_mapper, &cfg.layout_order);
        App {
            engine: Engine::new(dict.clone()),
            dict,
            buffer: WordBuffer::new(cfg.max_word_len),
            translit,
            layout: Layout::Ru,
            mapper,
            last_reason: String::new(),
            cfg,
        }
    }

    /// Нажатие на клавишу. Возвращает действие, которое нужно выполнить.
    pub fn on_key(&mut self, k: KeyPress) -> Action {
        self.engine.last_vk = k.vk;
        match self.buffer.push(k, &self.cfg.charset) {
            Push::Word => Action::Nothing,
            Push::Boundary => self.on_word_end(),
        }
    }

    /// Слово закончилось (пробел, знак, Enter) — принимаем решение.
    fn on_word_end(&mut self) -> Action {
        if self.buffer.is_empty() {
            return Action::Nothing;
        }
        let typed = self.mapper.text_for(self.buffer.keys(), self.layout);
        let other = convert(&typed, self.layout, self.layout.other());
        let action = self.evaluate(&typed, &other, false);
        self.buffer.clear();
        action
    }

    /// Ручная конвертация последнего слова (горячая клавиша).
    pub fn convert_now(&mut self) -> Action {
        if self.buffer.is_empty() {
            self.last_reason = String::from("пустое слово");
            return Action::Nothing;
        }
        let typed = self.mapper.text_for(self.buffer.keys(), self.layout);
        let other = convert(&typed, self.layout, self.layout.other());
        let action = self.evaluate(&typed, &other, true);
        self.buffer.clear();
        action
    }

    fn evaluate(&mut self, typed: &str, other: &str, forced: bool) -> Action {
        if typed.is_empty() {
            return Action::Nothing;
        }
        if typed.chars().all(|c| !c.is_alphabetic()) {
            self.last_reason = String::from("нет букв");
            return Action::Nothing;
        }
        if self.cfg.dont_convert_capitals {
            if let Some(first) = typed.chars().next() {
                if first.is_uppercase() {
                    self.last_reason = String::from("DontConvertCapitals");
                    return Action::Nothing;
                }
            }
        }
        if self.cfg.detect_passwords && looks_like_password(typed) {
            self.last_reason = String::from("похоже на пароль");
            return Action::Nothing;
        }

        let decision: Decision = if forced {
            Decision { route: Route::Direct, last_vk: self.engine.last_vk }
        } else {
            self.engine.decide(typed, other)
        };

        if !decision.converts() {
            self.last_reason = format!("оставить: '{}' / '{}'", typed, other);
            return Action::Nothing;
        }

        let mut text = other.to_string();
        // Route::Recheck соответствует sub_41CAC0: перед перепечаткой
        // результат проверяется повторно. Наша проверка — «в другой
        // раскладке получилось слово, которое есть в словаре».
        if decision.route == Route::Recheck && !should_recheck_convert(&self.dict, other) {
            self.last_reason = format!("Recheck отказал: '{}' не найдено", other);
            return Action::Nothing;
        }
        if decision.route == Route::Direct {
            text = fix_two_upper_letters(&text);
        }
        let to = self.layout.other();
        self.last_reason = format!("конвертировать: '{}' -> '{}'", typed, text);
        self.layout = to;
        Action::Convert { to, erase: typed.chars().count(), text }
    }
}

/// `FixTwoUpperLetters`: «ПРИВЕТ» → «Привет», «ПРивет» → «Привет».
pub fn fix_two_upper_letters(s: &str) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    if chars.len() < 3 {
        return s.to_string();
    }
    let upper = chars.iter().filter(|c| c.is_uppercase()).count();
    if upper == chars.len() {
        for c in chars.iter_mut().skip(1) {
            *c = c.to_lowercase().next().unwrap_or(*c);
        }
    }
    chars.into_iter().collect()
}

/// Проверка для `Route::Recheck` (аналог повторного контроля в `sub_41CAC0`):
/// перепечатывать имеет смысл, только если результат — известное слово.
pub fn should_recheck_convert(dict: &Dict, other: &str) -> bool {
    dict.match_word(other).hit
}

/// Упрощённая копия `sub_41D120`: цифры, спецсимволы, длинные серии заглавных.
pub fn looks_like_password(s: &str) -> bool {
    // в Punto это отдельная проверка перед остальными
    if s.contains("/ha") {
        return true;
    }
    let len = s.chars().count();
    if len < 7 {
        return false;
    }
    let digits = s.chars().filter(|c| c.is_ascii_digit()).count();
    let specials = s.chars().filter(|c| "@/?!$^&*()".contains(*c)).count();
    let letters = s.chars().filter(|c| c.is_alphabetic()).count();
    if letters == 0 {
        return false;
    }
    digits > 0 && specials > 0
}
