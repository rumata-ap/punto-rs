//! `punto doctor` — что доступно в этой среде и что нужно доустановить.
//!
//! Смысл: на Wayland «просто работает» не бывает, всё зависит от композитора.
//! Доктор перечисляет проверки и выдаёт готовую рекомендацию.

use crate::config::Config;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

pub struct Check {
    pub name: String,
    pub level: Level,
    pub detail: String,
}

impl Check {
    pub fn ok(name: &str, detail: impl Into<String>) -> Check {
        Check { name: name.into(), level: Level::Ok, detail: detail.into() }
    }
    pub fn warn(name: &str, detail: impl Into<String>) -> Check {
        Check { name: name.into(), level: Level::Warn, detail: detail.into() }
    }
    pub fn fail(name: &str, detail: impl Into<String>) -> Check {
        Check { name: name.into(), level: Level::Fail, detail: detail.into() }
    }
}

/// Тип сессии по переменным окружения.
pub fn session_kind() -> &'static str {
    match std::env::var("XDG_SESSION_TYPE") {
        Ok(v) if v.eq_ignore_ascii_case("wayland") => "wayland",
        Ok(v) if v.eq_ignore_ascii_case("x11") => "x11",
        Ok(v) if v.eq_ignore_ascii_case("tty") => "tty",
        _ => {
            if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                "wayland"
            } else if std::env::var_os("DISPLAY").is_some() {
                "x11"
            } else {
                "unknown"
            }
        }
    }
}

/// Рабочий стол по переменным окружения (`GNOME`, `KDE`, `sway`, `Hyprland`).
pub fn desktop() -> String {
    let cur = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if cur.is_empty() {
        std::env::var("DESKTOP_SESSION").unwrap_or_else(|_| "unknown".into())
    } else {
        cur
    }
}

#[cfg(unix)]
pub fn has(cmd: &str) -> bool {
    std::process::Command::new("sh")
        .args(["-c", &format!("command -v {} >/dev/null 2>&1", cmd)])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub fn has(_cmd: &str) -> bool {
    false
}

/// Полный набор проверок. Не запускает ничего разрушительного.
pub fn run(cfg: &Config) -> Vec<Check> {
    let mut out = Vec::new();
    let kind = session_kind();
    out.push(Check::ok("сессия", format!("{} ({})", kind, desktop())));

    // чтение клавиш
    if cfg!(target_os = "linux") {
        let readable = std::fs::read_dir("/dev/input").is_ok();
        if readable {
            out.push(Check::ok("/dev/input", "доступен"));
        } else {
            out.push(Check::fail(
                "/dev/input",
                "нет доступа: добавьте пользователя в группу input",
            ));
        }
        if std::path::Path::new("/dev/uinput").exists() {
            out.push(Check::ok("/dev/uinput", "устройство есть"));
        } else {
            out.push(Check::fail(
                "/dev/uinput",
                "нет устройства: загрузите модуль uinput и добавьте udev-правило",
            ));
        }
    } else {
        out.push(Check::warn("ОС", "не Linux: доступен только режим sim"));
    }

    // переключение раскладки
    let mut switch_tool = None;
    for tool in ["g3kb-switch", "xkb-switch", "swaymsg", "hyprctl"] {
        if has(tool) {
            switch_tool = Some(tool.to_string());
            break;
        }
    }
    match (&switch_tool, kind) {
        (Some(t), _) => out.push(Check::ok("переключение", format!("доступно через {}", t))),
        (None, "wayland") => {
            let de = desktop().to_lowercase();
            if de.contains("gnome") {
                out.push(Check::fail(
                    "переключение",
                    "GNOME Wayland: нужен g3kb-switch + расширение GNOME Shell \
                     (mutter не даёт CLI для смены раскладки)",
                ));
            } else if de.contains("kde") || de.contains("plasma") {
                out.push(Check::warn(
                    "переключение",
                    "KDE: поставьте qdbus и используйте org.kde.keyboard",
                ));
            } else {
                out.push(Check::fail("переключение", "не найдено инструмента"));
            }
        }
        (None, _) => out.push(Check::fail(
            "переключение",
            "поставьте xkb-switch (X11) или g3kb-switch (GNOME)",
        )),
    }

    // вставка текста
    let wtype = has("wtype");
    let mut checks = Vec::new();
    if wtype {
        checks.push("wtype");
    }
    if has("wl-copy") {
        checks.push("wl-copy");
    }
    if has("xdotool") {
        checks.push("xdotool");
    }
    if checks.is_empty() {
        out.push(Check::warn(
            "вставка",
            "внешних инструментов нет: останется uinput (нужна верная раскладка)",
        ));
    } else {
        out.push(Check::ok("вставка", checks.join(", ")));
    }
    if kind == "wayland" && wtype {
        out.push(Check::warn(
            "wtype",
            "на GNOME/mutter протокол virtual-keyboard не поддержан — wtype не сработает",
        ));
    }

    // итоговая рекомендация
    let strategy = crate::output::choose(cfg, switch_tool.is_some(), wtype && kind != "wayland");
    out.push(Check::ok(
        "стратегия",
        format!("{} (выбор: {})", strategy.name(), cfg.output),
    ));
    out
}
