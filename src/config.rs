//! Конфигурация: простой ini-подобный файл, без внешних зависимостей.
//!
//! ```text
//! [general]
//! max_word_len = 32
//! detect_passwords = true
//! dont_convert_capitals = false
//! convert_hotkey = 0       ; VK код (0 = выключено, 145 = ScrollLock, 19 = Pause)
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
    /// VK код горячей клавиши ручной конвертации.
    /// `0` — выключена. **Нельзя задавать буквы и цифры**: старый дефолт
    /// `70` — это VK_F, а не ScrollLock (тот `145`), и конвертация срабатывала
    /// на каждое «f»/«а» посреди слова, перемешивая раскладку.
    pub convert_hotkey: u16,
    /// Конвертировать автоматически на границе слова (пробел, знак, Enter).
    /// `false` — только по `convert_chord` или `convert_hotkey`.
    pub auto_convert: bool,
    /// Комбинация ручной конвертации, например `ctrl+shift` (любая сторона)
    /// или `rctrl+rshift` (только правые). Пусто — выключено. Доступны
    /// `ctrl`/`lctrl`/`rctrl`, `shift`/`lshift`/`rshift`, `alt`/`lalt`/`ralt`,
    /// `super`|`meta`/`lsuper`/`rsuper`. Третья клавиша между клавишами
    /// комбинации её отменяет, а повторное нажатие возвращает последнее слово
    /// в другую раскладку (циклически).
    pub convert_chord: String,
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
            // 0 = выключено. 70 — это VK_F (буква «f»/«а»), а не ScrollLock
            // (145): с прежним значением конвертация срабатывала прямо
            // посреди слова и перемешивала раскладку.
            convert_hotkey: 0,
            // автосмена по пробелу по умолчанию выключена: слово исправляется
            // только вручную (convert_chord / convert_hotkey)
            auto_convert: false,
            // Ctrl+Shift работает «из коробки»: конфиг можно не заводить.
            // Без стороны — подойдёт и левая, и правая пара; третья клавиша
            // между модификаторами отменяет комбинацию (Ctrl+Shift+V и т. п.).
            convert_chord: String::from("ctrl+shift"),
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

/// Путь к `config.ini`: `PUNTO_CONFIG` → `$XDG_CONFIG_HOME/punto-rs/config.ini`
/// → `~/.config/punto-rs/config.ini`.
pub fn config_path() -> PathBuf {
    match std::env::var_os("PUNTO_CONFIG") {
        Some(p) => PathBuf::from(p),
        None => {
            let base = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home().join(".config"));
            base.join("punto-rs").join("config.ini")
        }
    }
}

/// Записать/заменить значение ключа в ini-тексте, **сохранив комментарии**,
/// порядок строк и пустые строки.
///
/// Нужно трею: переключатель «Автоправка по пробелу» меняет `auto_convert`
/// в файле пользователя, не превращая его конфиг в машинный.
pub fn update_ini_text(text: &str, key: &str, value: &str) -> String {
    let mut out = String::with_capacity(text.len() + 32);
    let mut done = false;
    for line in text.lines() {
        let code = line.split(';').next().unwrap_or("");
        let replaced = !done
            && code
                .find('=')
                .map(|eq| code[..eq].trim() == key)
                .unwrap_or(false);
        if replaced {
            // комментарий после ';' и пробелы перед ним сохраняем как были
            let (comment, gap) = match line.find(';') {
                Some(semi) => {
                    let code = &line[..semi];
                    (&line[semi..], &code[code.trim_end().len()..])
                }
                None => ("", ""),
            };
            out.push_str(&format!("{} = {}{}{}", key, value, gap, comment));
            done = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !done {
        out.push_str(&format!("{} = {}\n", key, value));
    }
    out
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
                "auto_convert" => cfg.auto_convert = value.eq_ignore_ascii_case("true"),
                "convert_chord" => cfg.convert_chord = value.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_value_is_replaced_in_place() {
        let src = "; комментарий остаётся\nauto_convert = true\ndetect_passwords = true\n";
        let out = update_ini_text(src, "auto_convert", "false");
        assert!(out.contains("; комментарий остаётся"), "{}", out);
        assert!(out.contains("auto_convert = false"), "{}", out);
        assert!(out.contains("detect_passwords = true"), "прочие ключи не трогаем");
        assert!(!out.contains("auto_convert = true"));
    }

    #[test]
    fn ini_inline_comment_survives() {
        let src = "auto_convert = true   ; автосмена по пробелу\n";
        let out = update_ini_text(src, "auto_convert", "false");
        assert_eq!(out.trim_end(), "auto_convert = false   ; автосмена по пробелу");
    }

    #[test]
    fn ini_key_is_appended_when_missing() {
        let src = "[general]\nmax_word_len = 32\n";
        let out = update_ini_text(src, "auto_convert", "false");
        assert!(out.ends_with("auto_convert = false\n"), "{}", out);
        assert!(out.starts_with("[general]\nmax_word_len = 32\n"));
    }

    #[test]
    fn ini_only_first_occurrence_is_replaced() {
        let src = "dict = a\ndict = b\n";
        let out = update_ini_text(src, "dict", "c");
        assert_eq!(out, "dict = c\ndict = b\n");
    }

    /// Значение из правленого текста должно читаться конфигом.
    #[test]
    fn ini_roundtrip_is_loadable() {
        let dir = std::env::temp_dir().join(format!("punto-ini-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.ini");
        let text = update_ini_text(
            "[general]\nauto_convert = true ; комментарий\n",
            "auto_convert",
            "false",
        );
        std::fs::write(&path, text).unwrap();
        let cfg = Config::load(&path);
        assert!(!cfg.auto_convert, "правка должна выключать автоправку");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Свитчер обязан работать без конфига: ручная конвертация включена
    /// по умолчанию, иначе после чистой установки «ничего не происходит»
    /// и поведение приходится настраивать вручную.
    #[test]
    fn default_chord_is_ready_out_of_the_box() {
        let cfg = Config::default();
        assert_eq!(cfg.convert_chord, "ctrl+shift");
        assert!(!cfg.auto_convert, "по пробелу по умолчанию не трогаем");
        // отсутствующий файл — это те же дефолты, а не пустой конфиг
        let missing = Config::load(&PathBuf::from("/nonexistent/punto-rs/config.ini"));
        assert_eq!(missing.convert_chord, "ctrl+shift");
    }
}

