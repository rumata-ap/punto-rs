//! Конфигурация: простой ini-подобный файл, без внешних зависимостей.
//!
//! ```text
//! [general]
//! max_word_len = 32
//! detect_passwords = true
//! dont_convert_capitals = false
//! convert_hotkey = 70      ; VK код (70 = ScrollLock)
//! switch_command = xkb-switch -n
//! charset = abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_
//!
//! [files]
//! dict = ~/.config/punto-rs/en.dict
//! dict = ~/.config/punto-rs/ru.dict
//! dict = ~/.config/punto-rs/wrong.dict
//! ```

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub max_word_len: usize,
    pub detect_passwords: bool,
    pub dont_convert_capitals: bool,
    /// VK код горячей клавиши ручной конвертации (Punto: VK_CONVERT / Pause).
    pub convert_hotkey: u16,
    /// Команда смены раскладки на следующую (X11/Wayland — своя).
    pub switch_command: String,
    /// Команда получения текущей раскладки (пусто = автопоиск).
    pub layout_get_command: String,
    /// Команда установки конкретной раскладки, `{n}` — индекс (пусто = switch_command).
    pub layout_set_command: String,
    /// Порядок раскладок в системе: `en,ru`.
    pub layout_order: String,
    /// Источник «клавиша → символ»: `static` (встроенная таблица) или `xkb`.
    pub text_mapper: String,
    /// Как вставлять исправленный текст: `auto` | `uinput` | `clipboard` | `command`.
    pub output: String,
    /// Команда вставки текста для `output = command` (`{text}` — подстановка).
    pub type_command: String,
    /// Символы, из которых состоит слово.
    pub charset: String,
    pub dict_paths: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            max_word_len: 32,
            detect_passwords: true,
            dont_convert_capitals: false,
            convert_hotkey: 70, // VK_SCROLL
            switch_command: String::from("xkb-switch -n"),
            layout_get_command: String::new(),
            layout_set_command: String::new(),
            layout_order: String::from("en,ru"),
            text_mapper: String::from("static"),
            output: String::from("auto"),
            type_command: String::from("wtype {text}"),
            charset: String::from(
                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ\
                 абвгдеёжзийклмнопрстуфхцчшщъыьэюя\
                 АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ\
                 0123456789-_",
            ),
            dict_paths: vec![
                home().join(".config/punto-rs/ru.dict"),
                home().join(".config/punto-rs/en.dict"),
                home().join(".config/punto-rs/wrong.dict"),
            ],
        }
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

impl Config {
    pub fn load(path: &PathBuf) -> Config {
        let mut cfg = Config::default();
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => return cfg,
        };
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if line.is_empty() || line.starts_with('[') || line.starts_with('#') {
                continue;
            }
            let (key, value) = match line.split_once('=') {
                Some(kv) => (kv.0.trim(), kv.1.trim()),
                None => continue,
            };
            match key {
                "max_word_len" => {
                    if let Ok(v) = value.parse() {
                        cfg.max_word_len = v;
                    }
                }
                "detect_passwords" => cfg.detect_passwords = value.eq_ignore_ascii_case("true"),
                "dont_convert_capitals" => {
                    cfg.dont_convert_capitals = value.eq_ignore_ascii_case("true")
                }
                "convert_hotkey" => {
                    if let Ok(v) = value.parse() {
                        cfg.convert_hotkey = v;
                    }
                }
                "switch_command" => cfg.switch_command = value.to_string(),
                "layout_get_command" => cfg.layout_get_command = value.to_string(),
                "layout_set_command" => cfg.layout_set_command = value.to_string(),
                "layout_order" => cfg.layout_order = value.to_string(),
                "text_mapper" => cfg.text_mapper = value.to_string(),
                "output" => cfg.output = value.to_string(),
                "type_command" => cfg.type_command = value.to_string(),
                "charset" => cfg.charset = value.to_string(),
                "dict" => {
                    let p = value.strip_prefix("~/").map(|rest| home().join(rest));
                    cfg.dict_paths.push(p.unwrap_or_else(|| PathBuf::from(value)));
                }
                _ => {}
            }
        }
        cfg
    }
}
