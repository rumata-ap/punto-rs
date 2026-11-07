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
use std::sync::Arc;

/// Что должен сделать внешний слой (ввод-вывод).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Action {
    Nothing,
    /// переключить раскладку на `to`, стереть `erase` символов и напечатать `text`
    Convert { to: Layout, erase: usize, text: String },
}

/// Последнее перепечатанное слово — для циклической ручной конвертации.
///
/// В Punto повторное нажатие клавиши конвертации возвращает слово обратно
/// (и так по кругу). Чтобы вернуть именно то, что было, храним нажатия, из
/// которых слово собрано: посимвольная перекодировка текста потеряла бы
/// регистр (`Привет` → `GHBDTN` → `ПРИВЕТ`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastWord {
    /// нажатия, из которых собрано слово
    pub keys: Vec<KeyPress>,
    /// текст, который сейчас на экране
    pub text: String,
    /// раскладка, в которой он напечатан
    pub layout: Layout,
}

pub struct App {
    pub cfg: Config,
    pub dict: Arc<Dict>,
    pub engine: Engine,
    pub buffer: WordBuffer,
    pub translit: Translit,
    pub layout: Layout,
    /// источник «клавиша → символ» (встроенная таблица или xkbcommon)
    pub mapper: Mapper,
    /// почему последнее решение было таким (для логов)
    pub last_reason: String,
    /// последнее ручное исправление: база цикла «туда-обратно»
    pub last_word: Option<LastWord>,
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
        // на полных словарях hunspell копия словаря — это десятки мегабайт,
        // поэтому App и Engine делят одну и ту же запись через Arc
        let dict = Arc::new(dict);
        App {
            engine: Engine::new(Arc::clone(&dict)),
            dict,
            buffer: WordBuffer::new(cfg.max_word_len),
            translit,
            layout: Layout::Ru,
            mapper,
            last_reason: String::new(),
            last_word: None,
            cfg,
        }
    }

    /// Нажатие на клавишу. Возвращает действие, которое нужно выполнить.
    pub fn on_key(&mut self, k: KeyPress) -> Action {
        self.engine.last_vk = k.vk;
        // набрали что-то новое — цикл «туда-обратно» начинается заново:
        // вернуть предыдущее слово уже нельзя, курсор в другом месте
        self.last_word = None;
        match self.buffer.push(k, &self.cfg.charset) {
            Push::Word => Action::Nothing,
            Push::Boundary => {
                // auto_convert = false: на пробел/знак слово не трогаем —
                // конвертация только вручную (convert_chord / convert_hotkey)
                if !self.cfg.auto_convert {
                    self.buffer.clear();
                    self.last_reason = String::from("auto_convert выключен");
                    return Action::Nothing;
                }
                self.on_word_end()
            }
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
    ///
    /// Если буфер пуст, но слово уже перепечатывалось — возвращаем его в другую
    /// раскладку: повторное нажатие работает как переключатель «туда-обратно».
    pub fn convert_now(&mut self) -> Action {
        if self.buffer.is_empty() {
            return match self.last_word.clone() {
                Some(prev) => self.cycle(&prev),
                None => {
                    self.last_reason = String::from("пустое слово");
                    Action::Nothing
                }
            };
        }
        let keys: Vec<KeyPress> = self.buffer.keys().to_vec();
        let typed = self.mapper.text_for(&keys, self.layout);
        let other = convert(&typed, self.layout, self.layout.other());
        let action = self.evaluate(&typed, &other, true);
        // запоминаем базу для цикла только у ручной конвертации: при
        // auto_convert слово уже закрыто пробелом, и стирание съело бы
        // разделитель
        if let Action::Convert { to, text, .. } = &action {
            self.last_word = Some(LastWord { keys, text: text.clone(), layout: *to });
        }
        self.buffer.clear();
        action
    }

    /// Следующий шаг цикла: то же слово, но в другой раскладке.
    fn cycle(&mut self, prev: &LastWord) -> Action {
        let to = prev.layout.other();
        let text = fix_two_upper_letters(&self.mapper.text_for(&prev.keys, to));
        if text.is_empty() {
            self.last_reason = String::from("пустое слово");
            return Action::Nothing;
        }
        self.last_reason = format!("обратно: '{}' -> '{}'", prev.text, text);
        self.layout = to;
        self.last_word = Some(LastWord { keys: prev.keys.clone(), text: text.clone(), layout: to });
        Action::Convert { to, erase: prev.text.chars().count(), text }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with(auto_convert: bool) -> App {
        let mut cfg = Config::default();
        cfg.auto_convert = auto_convert;
        cfg.dict_paths.clear(); // словарь в этих тестах не нужен
        App::new(cfg)
    }

    /// «ghbdtn» — шесть нажатий, которые в EN дают "ghbdtn", в RU — "привет".
    fn press_word(app: &mut App) {
        for vk in [0x47, 0x48, 0x42, 0x44, 0x54, 0x4E] {
            app.on_key(KeyPress { vk, shift: false, caps: false });
        }
    }

    #[test]
    fn auto_convert_off_ignores_space_and_clears_buffer() {
        let mut app = app_with(false);
        press_word(&mut app);
        assert_eq!(app.buffer.len(), 6);
        let action = app.on_key(KeyPress { vk: 0x20, shift: false, caps: false });
        assert_eq!(action, Action::Nothing, "на пробеле конвертировать нельзя");
        assert_eq!(app.buffer.len(), 0);
        assert_eq!(app.last_reason, "auto_convert выключен");
    }

    #[test]
    fn manual_convert_works_on_typed_word() {
        let mut app = app_with(false);
        app.layout = Layout::En;
        press_word(&mut app);
        match app.convert_now() {
            Action::Convert { to, erase, text } => {
                assert_eq!(to, Layout::Ru);
                assert_eq!(erase, 6);
                assert_eq!(text, "привет");
            }
            Action::Nothing => panic!("ожидалась конвертация по ручной комбинации"),
        }
    }

    /// Повторное нажатие возвращает слово обратно — и так по кругу.
    #[test]
    fn manual_convert_cycles_back_and_forth() {
        let mut app = app_with(false);
        app.layout = Layout::En;
        press_word(&mut app);
        match app.convert_now() {
            Action::Convert { to, erase, text } => {
                assert_eq!((to, erase), (Layout::Ru, 6));
                assert_eq!(text, "привет");
            }
            Action::Nothing => panic!("первое нажатие должно конвертировать"),
        }
        // буфер уже пуст: второе нажатие возвращает слово как было набрано
        match app.convert_now() {
            Action::Convert { to, erase, text } => {
                assert_eq!((to, erase), (Layout::En, 6), "стираем ровно то, что напечатали");
                assert_eq!(text, "ghbdtn");
            }
            Action::Nothing => panic!("второе нажатие должно вернуть слово"),
        }
        assert_eq!(app.layout, Layout::En, "состояние следует за словом");
        // и снова в русскую — цикл замкнулся
        match app.convert_now() {
            Action::Convert { to, text, .. } => {
                assert_eq!(to, Layout::Ru);
                assert_eq!(text, "привет");
            }
            Action::Nothing => panic!("третий шаг цикла"),
        }
    }

    /// На каждом шаге цикла сохраняются правила регистра (`FixTwoUpperLetters`).
    #[test]
    fn cycle_applies_case_rules_on_each_step() {
        let mut app = app_with(false);
        app.layout = Layout::En;
        // «GHBDTN» набрано с Shift: в русской раскладке это «ПРИВЕТ»
        for vk in [0x47u16, 0x48, 0x42, 0x44, 0x54, 0x4E] {
            app.on_key(KeyPress { vk, shift: true, caps: false });
        }
        match app.convert_now() {
            Action::Convert { text, .. } => assert_eq!(text, "Привет"),
            Action::Nothing => panic!("первая конвертация"),
        }
        match app.convert_now() {
            Action::Convert { to, text, erase } => {
                assert_eq!(to, Layout::En);
                assert_eq!(erase, 6);
                assert_eq!(text, "Ghbdtn", "регистр не превращается в капс");
            }
            Action::Nothing => panic!("цикл должен вернуть слово"),
        }
    }

    /// Новый набор сбрасывает цикл: старое слово уже не «последнее».
    #[test]
    fn new_typing_breaks_the_cycle() {
        let mut app = app_with(false);
        app.layout = Layout::En;
        press_word(&mut app);
        assert!(matches!(app.convert_now(), Action::Convert { .. }));
        assert!(app.last_word.is_some());

        app.on_key(KeyPress { vk: 0x47, shift: false, caps: false });
        assert!(app.last_word.is_none(), "нажатие клавиши сбрасывает цикл");

        // пробел закрывает слово, буфер пуст и возвращать нечего
        let space = app.on_key(KeyPress { vk: 0x20, shift: false, caps: false });
        assert_eq!(space, Action::Nothing);
        assert_eq!(app.convert_now(), Action::Nothing);
        assert_eq!(app.last_reason, "пустое слово");
    }
}
