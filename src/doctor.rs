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

/// Проверка `/dev/input`: каталог читается и хотя бы одно `event*`
/// действительно открывается на чтение.
///
/// Существования каталога мало: он виден всем, а сами устройства — только
/// группе `input`, поэтому демон падал бы уже после старта.
pub fn check_input(dir: &std::path::Path) -> Check {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return Check::fail(
                "/dev/input",
                "нет доступа к каталогу: sudo usermod -aG input \"$USER\" и перелогиниться",
            );
        }
        Err(e) => return Check::fail("/dev/input", format!("не читается: {}", e)),
    };
    let (mut events, mut opened) = (0usize, 0usize);
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("event") {
            continue;
        }
        events += 1;
        if std::fs::File::open(entry.path()).is_ok() {
            opened += 1;
        }
    }
    if events == 0 {
        return Check::fail("/dev/input", "нет устройств event*: клавиатура не найдена");
    }
    if opened == 0 {
        return Check::fail(
            "/dev/input",
            "устройства event* не открываются: sudo usermod -aG input \"$USER\" и перелогиниться",
        );
    }
    Check::ok(
        "/dev/input",
        format!("доступно устройств: {} из {}", opened, events),
    )
}

/// Автозапуск при входе в систему: включён ли юнит `punto-rs` у user-менеджера.
pub fn check_autostart() -> Option<Check> {
    if !has("systemctl") {
        return None;
    }
    let out = std::process::Command::new("systemctl")
        .args(["--user", "is-enabled", "punto-rs"])
        .output()
        .ok()?;
    Some(autostart_check(&String::from_utf8_lossy(&out.stdout)))
}

/// Разбор ответа `systemctl --user is-enabled punto-rs`.
pub fn autostart_check(answer: &str) -> Check {
    match answer.trim() {
        "enabled" => Check::ok("автозапуск", "включён: systemd поднимет демон при входе"),
        "enabled-runtime" => Check::warn(
            "автозапуск",
            "включён только до перезагрузки: systemctl --user enable punto-rs",
        ),
        "" | "not-found" => Check::warn(
            "автозапуск",
            "юнит punto-rs не найден: install -Dm644 packaging/punto-rs.service \
             ~/.config/systemd/user/ && systemctl --user daemon-reload && \
             systemctl --user enable --now punto-rs",
        ),
        other => Check::warn(
            "автозапуск",
            format!("{} — включите: systemctl --user enable --now punto-rs", other),
        ),
    }
}

/// Трей: зарегистрирован ли в сессионной шине `org.kde.StatusNotifierWatcher`.
///
/// Без него иконка StatusNotifierItem не покажется: в KDE/плазма он штатный,
/// в GNOME нужен `gnome-shell-extension-appindicator`, в sway/Hyprland —
/// waybar с модулем `tray` (или `snixembed`).
pub fn check_tray() -> Option<Check> {
    if session_kind() == "tty" {
        return None;
    }
    let names = bus_names()?;
    Some(if has_watcher(&names) {
        Check::ok("трей", "StatusNotifierWatcher есть: `punto tray` покажет иконку")
    } else {
        Check::warn(
            "трей",
            "StatusNotifierWatcher не найден — иконка не появится; GNOME: \
             sudo apt install gnome-shell-extension-appindicator && \
             gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com",
        )
    })
}

/// Есть ли среди имён шины владелец StatusNotifierWatcher.
pub fn has_watcher(names: &str) -> bool {
    names.contains("StatusNotifierWatcher") || names.contains("AyatanaWatcher")
}

/// Имена объектов сессионной шины: `gdbus` (glib2, есть в GNOME), иначе
/// `busctl --user list` из systemd — вывод разный, но имена видны в обоих.
pub fn bus_names() -> Option<String> {
    let gdbus: [&str; 8] = [
        "call",
        "--session",
        "--dest",
        "org.freedesktop.DBus",
        "--object-path",
        "/org/freedesktop/DBus",
        "--method",
        "org.freedesktop.DBus.ListNames",
    ];
    if let Ok(out) = std::process::Command::new("gdbus").args(gdbus).output() {
        if out.status.success() {
            return Some(String::from_utf8_lossy(&out.stdout).into_owned());
        }
    }
    let out = std::process::Command::new("busctl")
        .args(["--user", "--no-pager", "list"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Проверка установки конкретной раскладки (`layout_set_command`).
///
/// Инструмент может быть установлен, а сама команда — не работать: на GNOME 48
/// `g3kb-switch -s N` отвечает `Failed to activate layout N` и выходит с кодом 1,
/// хотя `-n` работает. Проверяем идемпотентно — «ставим» ту раскладку, которая
/// и так активна. `None` — проверять нечего (команда не задана или раскладка
/// не определяется).
pub fn check_layout_set(cfg: &Config) -> Option<Check> {
    if cfg.layout_set_command.trim().is_empty() {
        return None;
    }
    let current = crate::layout::detect(cfg)?;
    let cmd = crate::layout::switch_command(cfg, current);
    Some(if crate::layout::run_command(&cmd) {
        Check::ok("установка раскладки", format!("`{}` работает", cmd))
    } else {
        Check::warn(
            "установка раскладки",
            format!(
                "`{}` не сработала — раскладка будет меняться через switch_command (`{}`)",
                cmd, cfg.switch_command
            ),
        )
    })
}

/// Проверка `/dev/uinput` **на запись**: именно запись нужна для инъекции.
///
/// По умолчанию устройство принадлежит `root:root` с правами `0600`, поэтому
/// без udev-правила `Injector::new()` падает с `Permission denied`, и демон
/// умирает на старте. Проверка «файл существует» этого не видит.
pub fn check_uinput(path: &std::path::Path) -> Check {
    match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(_) => Check::ok("/dev/uinput", "открывается на запись"),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Check::fail(
            "/dev/uinput",
            "нет прав на запись (обычно root:root 0600): sudo cp \
             packaging/99-punto-rs-uinput.rules /etc/udev/rules.d/ && \
             sudo udevadm control --reload && sudo udevadm trigger, затем перелогиниться; \
             разово — sudo chgrp input /dev/uinput && sudo chmod 660 /dev/uinput",
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Check::fail(
            "/dev/uinput",
            "нет устройства: sudo modprobe uinput и правило из packaging/",
        ),
        Err(e) => Check::fail("/dev/uinput", format!("не открывается на запись: {}", e)),
    }
}

/// Полный набор проверок. Не запускает ничего разрушительного.
pub fn run(cfg: &Config) -> Vec<Check> {
    let mut out = Vec::new();
    let kind = session_kind();
    out.push(Check::ok("сессия", format!("{} ({})", kind, desktop())));

    // автозапуск: демона не будет после следующего входа, если юнит не включён
    if cfg!(unix) {
        if let Some(c) = check_autostart() {
            out.push(c);
        }
    }

    // чтение клавиш и инъекция
    if cfg!(target_os = "linux") {
        out.push(check_input(std::path::Path::new("/dev/input")));
        out.push(check_uinput(std::path::Path::new("/dev/uinput")));
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

    // установка конкретной раскладки: инструмент может быть, а команда — нет
    if cfg!(unix) {
        if let Some(c) = check_layout_set(cfg) {
            out.push(c);
        }
    }

    // трей: есть ли в сессии StatusNotifierWatcher
    if cfg!(unix) {
        if let Some(c) = check_tray() {
            out.push(c);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("punto-doctor-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writable_uinput_is_ok() {
        let dir = tmpdir("uinput-ok");
        let p = dir.join("uinput");
        std::fs::write(&p, b"").unwrap();
        let c = check_uinput(&p);
        assert_eq!(c.level, Level::Ok, "{}", c.detail);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_uinput_points_to_module() {
        let dir = tmpdir("uinput-none");
        let c = check_uinput(&dir.join("uinput"));
        assert_eq!(c.level, Level::Fail);
        assert!(c.detail.contains("modprobe uinput"), "{}", c.detail);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `/dev/uinput` по умолчанию `root:root 0600` — доктор обязан это увидеть,
    /// иначе он пишет «устройство есть», а демон падает с `Permission denied`.
    #[cfg(unix)]
    #[test]
    fn unwritable_uinput_points_to_udev_rule() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmpdir("uinput-denied");
        let p = dir.join("uinput");
        std::fs::write(&p, b"").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o400)).unwrap();
        // под root права не проверяются — тогда проверять нечего
        if std::fs::OpenOptions::new().write(true).open(&p).is_ok() {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let c = check_uinput(&p);
        assert_eq!(c.level, Level::Fail, "{}", c.detail);
        assert!(c.detail.contains("99-punto-rs-uinput.rules"), "{}", c.detail);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn readable_event_devices_are_counted() {
        let dir = tmpdir("input-ok");
        std::fs::write(dir.join("event0"), b"").unwrap();
        std::fs::write(dir.join("event1"), b"").unwrap();
        std::fs::write(dir.join("mice"), b"").unwrap(); // не event* — не считаем
        let c = check_input(&dir);
        assert_eq!(c.level, Level::Ok, "{}", c.detail);
        assert!(c.detail.contains("2 из 2"), "{}", c.detail);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn input_without_event_devices_fails() {
        let dir = tmpdir("input-empty");
        let c = check_input(&dir);
        assert_eq!(c.level, Level::Fail);
        assert!(c.detail.contains("клавиатура не найдена"), "{}", c.detail);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn autostart_states_are_classified() {
        assert_eq!(autostart_check("enabled\n").level, Level::Ok);
        assert_eq!(autostart_check("enabled-runtime").level, Level::Warn);
        assert_eq!(autostart_check("disabled").level, Level::Warn);
        assert_eq!(autostart_check("not-found").level, Level::Warn);
        assert!(
            autostart_check("").detail.contains("packaging/punto-rs.service"),
            "пустой ответ = юнит не найден, нужна подсказка про установку"
        );
    }

    #[test]
    fn watcher_detection_matches_real_bus_dumps() {
        // вывод gdbus: (['org.freedesktop.DBus', ':1.7', ...])
        let gdbus = "(['org.freedesktop.DBus', 'org.kde.StatusNotifierWatcher'])";
        assert!(has_watcher(gdbus));
        // вывод busctl --user list: колонки NAME PID PROCESS ...
        assert!(has_watcher("org.kde.StatusNotifierWatcher 1234 app"));
        assert!(has_watcher("org.ayatana.StatusNotifierWatcher"));
        assert!(!has_watcher("org.freedesktop.Notifications"));
    }
}
