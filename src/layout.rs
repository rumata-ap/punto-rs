//! Определение текущей раскладки и переключение — под X11, GNOME/Wayland, KDE.
//!
//! В Punto раскладка берётся из Win32 (`GetKeyboardLayout`). На Linux единого
//! API нет, поэтому используется набор стратегий, проверяемых по очереди:
//!
//! | среда | получение | переключение |
//! |---|---|---|
//! | X11 | `xkb-switch -p` | `xkb-switch -n` / `-s N` |
//! | GNOME (Wayland тоже) | `g3kb-switch -p` | `g3kb-switch -n` / `-s N` |
//! | KDE | `qdbus org.kde.keyboard /Layouts …getLayout` | `…setLayout N` |
//! | GNOME без расширения | `gsettings get … mru-sources` | не гарантируется |
//!
//! Любую команду можно переопределить в конфиге.

use crate::config::Config;
use crate::keymap::Layout;

#[cfg(unix)]
use std::process::Command;

/// Порядок раскладок в системе (`layout_order = en,ru`).
pub fn order(cfg: &Config) -> Vec<Layout> {
    let mut out = Vec::new();
    for part in cfg.layout_order.split(',') {
        match part.trim().to_ascii_lowercase().as_str() {
            "ru" => out.push(Layout::Ru),
            "en" | "us" => out.push(Layout::En),
            _ => {}
        }
    }
    if out.is_empty() {
        out.push(Layout::En);
        out.push(Layout::Ru);
    }
    out
}

pub fn index_of(cfg: &Config, layout: Layout) -> usize {
    order(cfg).iter().position(|l| *l == layout).unwrap_or(0)
}

/// Разбор ответа любого пробора: `0`, `1`, `us`, `ru`, `Russian`, `('xkb','ru')`.
pub fn parse_layout(text: &str) -> Option<Layout> {
    let low = text.to_lowercase();
    if let Ok(n) = low.trim().parse::<usize>() {
        // индекс в порядке раскладок; 0 — первая (обычно en)
        return match n {
            0 => Some(Layout::En),
            _ => Some(Layout::Ru),
        };
    }
    if low.contains("ru") || low.contains("russian") || low.contains("русск") {
        return Some(Layout::Ru);
    }
    if low.contains("us") || low.contains("en") || low.contains("english")
        || low.contains("англ")
    {
        return Some(Layout::En);
    }
    None
}

/// `gsettings get org.gnome.desktop.input-sources mru-sources` →
/// `[<('xkb', 'ru')>, <('xkb', 'us')>]`; берём первую (самую свежую).
pub fn parse_gsettings_mru(text: &str) -> Option<Layout> {
    let first = text.split("('xkb'").nth(1)?;
    let code = first.split('\'') .nth(1)?;
    parse_layout(code)
}

#[cfg(unix)]
fn run(cmd: &str) -> Option<String> {
    let out = Command::new("sh").args(["-c", cmd]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(not(unix))]
fn run(_cmd: &str) -> Option<String> {
    None
}

/// Команды-проборы по умолчанию: первая успешная считается ответом.
fn defaults() -> [&'static str; 4] {
    [
        "g3kb-switch -p",
        "xkb-switch -p",
        "qdbus org.kde.keyboard /Layouts org.kde.KeyboardLayouts.getLayout",
        "gsettings get org.gnome.desktop.input-sources mru-sources",
    ]
}

/// Текущая раскладка: своя команда из конфига либо автопоиск.
pub fn detect(cfg: &Config) -> Option<Layout> {
    if !cfg.layout_get_command.trim().is_empty() {
        let text = run(&cfg.layout_get_command)?;
        return if text.contains("mru-sources") || text.starts_with('[') {
            parse_gsettings_mru(&text)
        } else {
            parse_layout(&text)
        };
    }
    for cmd in defaults() {
        if let Some(text) = run(cmd) {
            let res = if cmd.contains("gsettings") {
                parse_gsettings_mru(&text)
            } else {
                parse_layout(&text)
            };
            if res.is_some() {
                return res;
            }
        }
    }
    None
}

/// Команда переключения на конкретную раскладку (`{n}` — индекс).
pub fn switch_command(cfg: &Config, target: Layout) -> String {
    let idx = index_of(cfg, target);
    if !cfg.layout_set_command.trim().is_empty() {
        return cfg.layout_set_command.replace("{n}", &idx.to_string());
    }
    // если задана только «следующая» — используем её
    cfg.switch_command.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_variants() {
        assert_eq!(parse_layout("0"), Some(Layout::En));
        assert_eq!(parse_layout("1"), Some(Layout::Ru));
        assert_eq!(parse_layout("us"), Some(Layout::En));
        assert_eq!(parse_layout("ru"), Some(Layout::Ru));
        assert_eq!(parse_layout("Russian"), Some(Layout::Ru));
        assert_eq!(parse_layout("English (US)"), Some(Layout::En));
        assert_eq!(parse_layout("какая-то ерунда"), None);
    }

    #[test]
    fn parse_gnome_mru() {
        let s = "[<('xkb', 'ru')>, <('xkb', 'us')>]";
        assert_eq!(parse_gsettings_mru(s), Some(Layout::Ru));
        let s2 = "[<('xkb', 'us')>, <('xkb', 'ru')>]";
        assert_eq!(parse_gsettings_mru(s2), Some(Layout::En));
    }

    #[test]
    fn order_and_index() {
        let cfg = Config::default();
        assert_eq!(order(&cfg), vec![Layout::En, Layout::Ru]);
        assert_eq!(index_of(&cfg, Layout::Ru), 1);
        assert_eq!(index_of(&cfg, Layout::En), 0);
    }

    #[test]
    fn set_command_placeholder() {
        let mut cfg = Config::default();
        cfg.layout_set_command = "g3kb-switch -s {n}".into();
        assert_eq!(switch_command(&cfg, Layout::Ru), "g3kb-switch -s 1");
        cfg.layout_set_command = String::new();
        cfg.switch_command = "xkb-switch -n".into();
        assert_eq!(switch_command(&cfg, Layout::Ru), "xkb-switch -n");
    }
}
