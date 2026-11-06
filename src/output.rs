//! Как вставлять исправленный текст — выбор стратегии под конкретную среду.
//!
//! Проверенные факты (GNOME/mutter):
//!
//! * протокол `virtual-keyboard-unstable-v1` **не поддержан** (mutter #1974),
//!   поэтому `wtype` на GNOME Wayland не работает;
//! * `uinput` работает везде, но выдаёт символы **по текущей раскладке**,
//!   значит печатать можно только после переключения;
//! * буфер обмена (`wl-copy` + Ctrl+V через uinput) работает независимо от
//!   раскладки — это резерв, когда переключение недоступно.
//!
//! Отсюда порядок выбора: wtype (wlroots) → uinput после переключения →
//! буфер обмена.

use crate::config::Config;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Strategy {
    /// своя виртуальная клавиатура (uinput); требует верной раскладки
    Uinput,
    /// буфер обмена + Ctrl+V через uinput; раскладка не важна
    Clipboard,
    /// внешняя команда с `{text}` (например `wtype`)
    Command(String),
}

impl Strategy {
    pub fn name(&self) -> &'static str {
        match self {
            Strategy::Uinput => "uinput",
            Strategy::Clipboard => "clipboard",
            Strategy::Command(_) => "command",
        }
    }
}

/// Выбор стратегии: явная из конфига либо автоматическая.
///
/// `switch_ok` — доступно ли переключение раскладки (нашлась рабочая команда).
/// `wtype_ok` — есть ли `wtype` (работает только на wlroots-композиторах).
pub fn choose(cfg: &Config, switch_ok: bool, wtype_ok: bool) -> Strategy {
    match cfg.output.trim().to_ascii_lowercase().as_str() {
        "uinput" => return Strategy::Uinput,
        "clipboard" => return Strategy::Clipboard,
        "command" => return Strategy::Command(cfg.type_command.clone()),
        _ => {}
    }
    // auto
    if wtype_ok {
        // wtype печатает текст напрямую, раскладка не нужна
        return Strategy::Command(cfg.type_command.clone());
    }
    if switch_ok {
        return Strategy::Uinput;
    }
    Strategy::Clipboard
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config::default()
    }

    #[test]
    fn explicit_wins() {
        let mut c = cfg();
        c.output = "clipboard".into();
        assert_eq!(choose(&c, true, true), Strategy::Clipboard);
        c.output = "uinput".into();
        assert_eq!(choose(&c, true, true), Strategy::Uinput);
    }

    #[test]
    fn auto_prefers_wtype_then_switch_then_clipboard() {
        let c = cfg();
        assert!(matches!(choose(&c, true, true), Strategy::Command(_)));
        assert_eq!(choose(&c, true, false), Strategy::Uinput);
        assert_eq!(choose(&c, false, false), Strategy::Clipboard);
    }
}
