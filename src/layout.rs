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
use std::process::{Command, Stdio};

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

/// Выполнить команду: важен только признак успеха, вывод не нужен.
///
/// Вывод глушим сознательно: неудачная попытка `g3kb-switch -s N` — ожидаемая
/// ситуация (демон сразу пробует запасную команду), а её сообщение
/// `Failed to activate layout N` в журнале только путает. Если не сработали обе
/// команды, `serve()` напишет об этом сам.
#[cfg(unix)]
pub fn run_command(cmd: &str) -> bool {
    Command::new("sh")
        .args(["-c", cmd])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub fn run_command(_cmd: &str) -> bool {
    false
}

/// Совпало ли системное состояние с ожидаемым.
/// Определить раскладку нечем — считаем, что команда сработала.
fn matches_state(cfg: &Config, target: Layout) -> bool {
    detect(cfg).map(|l| l == target).unwrap_or(true)
}

/// Переключить раскладку на `target`, с запасным путём.
///
/// Практика (GNOME 48, `g3kb-switch` 1.5): `g3kb-switch -s 0` и `-s 1`
/// отвечают `Failed to activate layout 0: <no details>` и выходят с кодом 1,
/// хотя `-n` («следующая») работает. Поэтому при неудаче установки конкретной
/// раскладки пробуем `switch_command` — при двух раскладках это то же самое.
///
/// `true` — раскладка в итоге стала `target` (или проверить нечем, и хоть одна
/// команда отработала успешно).
pub fn switch_to(cfg: &Config, target: Layout) -> bool {
    // Уже в целевой раскладке — переключать нечего. Без этой проверки
    // `switch_command` (`g3kb-switch -n`) уводил в соседнюю раскладку,
    // проверка состояния не совпадала, и демон писал «слово не тронуто»:
    // слово оставалось как было, хотя делать ничего и не требовалось.
    if let Some(current) = detect(cfg) {
        if current == target {
            return true;
        }
    }
    let set_cmd = switch_command(cfg, target);
    if !set_cmd.trim().is_empty() && run_command(&set_cmd) && matches_state(cfg, target) {
        return true;
    }
    let next = cfg.switch_command.trim().to_string();
    if next.is_empty() || next == set_cmd {
        return false;
    }
    run_command(&next) && matches_state(cfg, target)
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

    /// `g3kb-switch -s N` на GNOME 48 падает, а `-n` работает: демон обязан
    /// откатиться на «следующую раскладку», иначе слово не исправить вообще.
    #[cfg(unix)]
    #[test]
    fn switch_to_falls_back_to_next_when_set_fails() {
        let mut cfg = Config::default();
        cfg.layout_get_command = "echo ru".into();
        cfg.layout_set_command = "false".into();
        cfg.switch_command = "true".into();
        assert!(switch_to(&cfg, Layout::Ru), "должен сработать запасной -n");
        // раскладка осталась us, значит переключения не было
        cfg.layout_get_command = "echo us".into();
        assert!(!switch_to(&cfg, Layout::Ru));
    }

    #[cfg(unix)]
    #[test]
    fn switch_to_prefers_set_command() {
        let mut cfg = Config::default();
        cfg.layout_get_command = "echo us".into();
        cfg.layout_set_command = "true".into();
        cfg.switch_command = "false".into();
        assert!(switch_to(&cfg, Layout::En));
        assert!(!switch_to(&cfg, Layout::Ru), "set и next не сработали");
    }

    #[cfg(unix)]
    #[test]
    fn switch_to_without_detect_trusts_command() {
        let mut cfg = Config::default();
        cfg.layout_get_command = "false".into(); // определить нечем
        cfg.layout_set_command = "true".into();
        assert!(switch_to(&cfg, Layout::Ru));
        cfg.layout_set_command = "false".into();
        cfg.switch_command = "false".into();
        assert!(!switch_to(&cfg, Layout::Ru));
    }

    /// Раскладка уже целевая — переключать нечего.
    ///
    /// Именно здесь демон молчал в реальной жизни: `g3kb-switch -s N` на
    /// GNOME 48 падает, запасной `-n` уводит в соседнюю раскладку, проверка
    /// состояния не совпадает — и слово оставалось нетронутым.
    #[cfg(unix)]
    #[test]
    fn switch_to_is_noop_when_already_there() {
        let mut cfg = Config::default();
        cfg.layout_get_command = "echo ru".into();
        cfg.layout_set_command = "false".into();
        cfg.switch_command = "false".into();
        assert!(switch_to(&cfg, Layout::Ru), "раскладка уже ru");
    }
}
