//! Иконка в системном трее — как у Punto Switcher в Windows.
//!
//! Реализован KDE/freedesktop StatusNotifierItem напрямую по D-Bus (крейт
//! `ksni`), без GTK и `libappindicator`: демону не нужен графический тулчейн.
//!
//! ```text
//! cargo build --release --features "input-evdev,tray"
//! punto tray
//! ```
//!
//! На GNOME иконки SNI показывает только расширение AppIndicator
//! (`gnome-shell-extension-appindicator`) — `punto doctor` предупредит,
//! если его нет.
//!
//! Меню повторяет Windows-версию: состояние демона и раскладки, переключатель
//! автоправки по пробелу, запуск/остановка демона, ручное преобразование
//! последнего слова, проверка среды (doctor), конфиг, журнал, выход.

use crate::config::{self, Config};
use crate::keymap::Layout;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, MenuItem, StandardItem, SubMenu};
use ksni::{Icon, Status, Tray};

const SERVICE: &str = "punto-rs.service";
const APPINDICATOR_EXT: &str = "appindicatorsupport@rgcjonas.gmail.com";

/// Что сделать по выбору пункта меню.
///
/// Обработчики пунктов только отправляют команду: внутри них нельзя
/// блокироваться, иначе замерзает меню. Работу делает отдельный поток.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmd {
    ToggleDaemon,
    ToggleAutoConvert,
    Restart,
    ConvertLast,
    Doctor,
    OpenConfig,
    OpenLog,
    Quit,
}

pub struct PuntoTray {
    cfg_path: PathBuf,
    cfg: Config,
    /// демон `punto-rs.service` активен
    daemon: bool,
    /// текущая раскладка: `ru` / `en` / `?`
    layout: &'static str,
    /// комбинация ручной конвертации задана
    chord: bool,
    /// последний результат действия или ошибка (показывается в меню)
    note: String,
    tx: Sender<Cmd>,
}

impl PuntoTray {
    pub fn new(cfg_path: PathBuf, tx: Sender<Cmd>) -> PuntoTray {
        let mut tray = PuntoTray {
            cfg_path,
            cfg: Config::default(),
            daemon: false,
            layout: "?",
            chord: false,
            note: String::new(),
            tx,
        };
        tray.reload();
        tray
    }

    /// Перечитать состояние с диска и из systemd (вызывается раз в 2 с).
    fn reload(&mut self) {
        self.cfg = Config::load(&self.cfg_path);
        self.daemon = service_active();
        self.layout = match crate::layout::detect(&self.cfg) {
            Some(Layout::Ru) => "ru",
            Some(Layout::En) => "en",
            None => "?",
        };
        self.chord = !self.cfg.convert_chord.trim().is_empty();
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }

    fn status_line(&self) -> String {
        if self.daemon {
            format!("punto-rs: работает · раскладка {}", self.layout)
        } else {
            String::from("punto-rs: остановлен")
        }
    }
}

/// Пункт меню только для чтения (заголовок/подсказка).
fn label(text: impl Into<String>) -> MenuItem<PuntoTray> {
    StandardItem { label: text.into(), enabled: false, ..Default::default() }.into()
}

impl Tray for PuntoTray {
    fn id(&self) -> String {
        String::from("punto-rs")
    }

    fn title(&self) -> String {
        String::from("punto-rs")
    }

    fn status(&self) -> Status {
        if self.daemon {
            Status::Active
        } else {
            Status::Passive
        }
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![tray_icon(self.daemon)]
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items = vec![label(self.status_line())];

        let mut info = format!(
            "вывод: {} · автоправка: {}",
            self.cfg.output,
            if self.cfg.auto_convert { "вкл" } else { "выкл" }
        );
        if !self.chord {
            info.push_str(" · ручная: не задана");
        }
        items.push(label(info));
        if !self.note.is_empty() {
            items.push(label(self.note.clone()));
        }

        items.push(
            CheckmarkItem {
                label: String::from("Демон запущен"),
                checked: self.daemon,
                activate: Box::new(|t: &mut Self| t.send(Cmd::ToggleDaemon)),
                ..Default::default()
            }
            .into(),
        );
        items.push(
            CheckmarkItem {
                label: String::from("Автоправка по пробелу"),
                checked: self.cfg.auto_convert,
                activate: Box::new(|t: &mut Self| t.send(Cmd::ToggleAutoConvert)),
                ..Default::default()
            }
            .into(),
        );

        let mut more = vec![
            StandardItem {
                label: String::from("Проверить среду (doctor)"),
                activate: Box::new(|t: &mut Self| t.send(Cmd::Doctor)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: String::from("Настройки (config.ini)"),
                activate: Box::new(|t: &mut Self| t.send(Cmd::OpenConfig)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: String::from("Журнал демона"),
                activate: Box::new(|t: &mut Self| t.send(Cmd::OpenLog)),
                ..Default::default()
            }
            .into(),
        ];
        if self.daemon && self.chord {
            more.insert(
                0,
                StandardItem {
                    label: format!("Преобразовать слово ({})", self.cfg.convert_chord),
                    activate: Box::new(|t: &mut Self| t.send(Cmd::ConvertLast)),
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(
            SubMenu { label: String::from("Ещё"), submenu: more, ..Default::default() }.into(),
        );

        items.push(
            StandardItem {
                label: String::from("Перезапустить демон"),
                activate: Box::new(|t: &mut Self| t.send(Cmd::Restart)),
                ..Default::default()
            }
            .into(),
        );
        items.push(
            StandardItem {
                label: String::from("Выход"),
                activate: Box::new(|t: &mut Self| t.send(Cmd::Quit)),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

/// Иконка 22×22: две половины (синяя — EN, красная — RU) в рамке;
/// серый вариант, когда демон остановлен.
fn tray_icon(active: bool) -> Icon {
    const N: i32 = 22;
    let center = N / 2;
    let mut data = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let dx = x - center;
            let dy = y - center;
            let inside = dx.abs() <= center - 1 && dy.abs() <= center - 1;
            let border = dx.abs() == center - 1 || dy.abs() == center - 1;
            // ARGB32, сетевой порядок байт
            let px: [u8; 4] = if !inside {
                [0, 0, 0, 0]
            } else if border {
                [255, 255, 255, 255]
            } else if !active {
                [255, 0x77, 0x77, 0x77]
            } else if dx < 0 {
                [255, 0x35, 0x65, 0xA4]
            } else if dx == 0 {
                [255, 0xFF, 0xFF, 0xFF]
            } else {
                [255, 0xCC, 0x00, 0x00]
            };
            data.extend_from_slice(&px);
        }
    }
    Icon { width: N, height: N, data }
}

/// Активен ли `punto-rs.service` у пользовательского systemd.
pub fn service_active() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", SERVICE])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Действие над сервисом: `start` / `stop` / `restart`.
pub fn service(cmd: &str) -> bool {
    Command::new("systemctl")
        .args(["--user", cmd, SERVICE])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Переключить `auto_convert` в конфиге пользователя, сохранив комментарии.
fn toggle_auto_convert(path: &Path) -> Result<bool, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("нет {}: {}", path.display(), e))?;
    let now = Config::load(&path.to_path_buf()).auto_convert;
    let next = !now;
    let updated = config::update_ini_text(&text, "auto_convert", if next { "true" } else { "false" });
    std::fs::write(path, updated).map_err(|e| format!("не записалось: {}", e))?;
    Ok(next)
}

/// Показать сообщение пользователю: `notify-send`, иначе в stdout.
fn notify(text: &str) {
    let ok = Command::new("notify-send")
        .args(["-a", "punto-rs", "-i", "input-keyboard", "punto-rs", text])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("punto-rs: {}", text);
    }
}

/// Текст `punto doctor` — для уведомления из меню.
fn doctor_text() -> String {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("punto"));
    match Command::new(exe).arg("doctor").output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).to_string();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            text.trim().to_string()
        }
        Err(e) => format!("не удалось запустить doctor: {}", e),
    }
}

/// Открыть журнал демона в терминале.
fn open_log() -> bool {
    let log = "journalctl --user -u punto-rs -n 200";
    let attempts: [(&str, [&str; 4]); 3] = [
        ("gnome-terminal", ["--", "sh", "-c", log]),
        ("konsole", ["-e", "sh", "-c", log]),
        ("x-terminal-emulator", ["-e", "sh", "-c", log]),
    ];
    for (term, args) in attempts {
        let ok = Command::new(term)
            .args(args)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return true;
        }
    }
    false
}

/// Постоянная uinput-клавиатура для инъекции комбинации ручной конвертации.
///
/// Имя отличается от демонской («punto-rs virtual keyboard»), поэтому демон
/// слушает её как обычную клавиатуру.
///
/// Устройство создаётся один раз и живёт вместе с треем: раньше оно делалось на
/// каждый клик и в него стреляли через 40 мс. Демон ещё не успевал открыть
/// новый `/dev/input/event*` (чтение блокирующее, опрос — раз в 3 секунды), и
/// события просто терялись: пункт меню «Преобразовать слово» не делал ничего.
#[cfg(all(target_os = "linux", feature = "input-evdev"))]
struct ChordInjector {
    dev: evdev::uinput::VirtualDevice,
    first: u16,
    second: u16,
}

#[cfg(all(target_os = "linux", feature = "input-evdev"))]
impl ChordInjector {
    fn new(cfg: &Config) -> Result<ChordInjector, String> {
        let chord = crate::io::parse_chord(&cfg.convert_chord)
            .ok_or_else(|| String::from("convert_chord не разобран: нужны две клавиши"))?;
        let (first, second) = chord.inject();
        let mut keys = evdev::AttributeSet::<evdev::Key>::new();
        keys.insert(evdev::Key::new(first));
        keys.insert(evdev::Key::new(second));
        // Демон считает клавиатурой устройство, у которого есть буквы и пробел
        // (`is_keyboard`). Без них он эту клавиатуру не откроет и комбинацию не
        // увидит — объявляем «настоящий» набор, хотя шлём только модификаторы.
        keys.insert(evdev::Key::KEY_A);
        keys.insert(evdev::Key::KEY_SPACE);
        let dev = evdev::uinput::VirtualDeviceBuilder::new()
            .map_err(|e| format!("/dev/uinput: {}", e))?
            .name("punto-rs tray")
            .with_keys(&keys)
            .map_err(|e| format!("/dev/uinput: {}", e))?
            .build()
            .map_err(|e| format!("/dev/uinput: {}", e))?;
        Ok(ChordInjector { dev, first, second })
    }

    /// Нажать оба модификатора и отпустить — ровно то, чего ждёт `ChordState`.
    fn fire(&mut self) -> Result<(), String> {
        let ev = |code: u16, value: i32| evdev::InputEvent::new(evdev::EventType::KEY, code, value);
        self.dev
            .emit(&[ev(self.first, 1), ev(self.second, 1)])
            .map_err(|e| format!("инъекция: {}", e))?;
        std::thread::sleep(std::time::Duration::from_millis(60));
        self.dev
            .emit(&[ev(self.second, 0), ev(self.first, 0)])
            .map_err(|e| format!("инъекция: {}", e))?;
        Ok(())
    }
}

#[cfg(not(all(target_os = "linux", feature = "input-evdev")))]
struct ChordInjector;

#[cfg(not(all(target_os = "linux", feature = "input-evdev")))]
impl ChordInjector {
    fn new(_cfg: &Config) -> Result<ChordInjector, String> {
        Err(String::from("нужна сборка с --features input-evdev"))
    }

    fn fire(&mut self) -> Result<(), String> {
        Err(String::from("нужна сборка с --features input-evdev"))
    }
}

/// Обработчик команд меню: единственное место, где можно блокироваться.
fn worker(rx: Receiver<Cmd>, handle: Handle<PuntoTray>, cfg_path: PathBuf) {
    // Устройство инъекции готовим сразу: демон должен успеть открыть его до
    // первого клика по меню, иначе события теряются (см. `ChordInjector`).
    let mut injector = ChordInjector::new(&Config::load(&cfg_path)).ok();
    for cmd in rx {
        let mut note = String::new();
        match cmd {
            Cmd::Quit => {
                handle.shutdown();
                return;
            }
            Cmd::ToggleDaemon => {
                let ok = if service_active() { service("stop") } else { service("start") };
                if !ok {
                    note = String::from("systemctl не сработал");
                }
            }
            Cmd::ToggleAutoConvert => match toggle_auto_convert(&cfg_path) {
                Ok(next) => {
                    // демон читает конфиг при старте — перечитываем его
                    service("restart");
                    note = format!("автоправка по пробелу: {}", if next { "вкл" } else { "выкл" });
                }
                Err(e) => note = e,
            },
            Cmd::Restart => {
                if !service("restart") {
                    note = String::from("перезапустить не удалось");
                }
            }
            Cmd::ConvertLast => {
                if injector.is_none() {
                    // конфиг могли поправить после старта трея
                    match ChordInjector::new(&Config::load(&cfg_path)) {
                        Ok(i) => injector = Some(i),
                        Err(e) => note = e,
                    }
                }
                if let Some(i) = injector.as_mut() {
                    if let Err(e) = i.fire() {
                        note = e;
                    }
                }
            }
            Cmd::Doctor => notify(&doctor_text()),
            Cmd::OpenConfig => {
                let _ = Command::new("xdg-open").arg(&cfg_path).status();
            }
            Cmd::OpenLog => {
                if !open_log() {
                    note = String::from("терминал не найден: journalctl --user -u punto-rs");
                }
            }
        }
        let keep = handle.update(|t: &mut PuntoTray| {
            t.reload();
            t.note = note.clone();
        });
        if keep.is_none() {
            return;
        }
    }
}

/// Запустить трей и обрабатывать меню до «Выход».
pub fn run() -> i32 {
    let cfg_path = config::config_path();
    let (tx, rx) = channel();
    let tray = PuntoTray::new(cfg_path.clone(), tx);
    let handle = match tray.spawn() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("punto-rs: иконка в трее недоступна: {}", e);
            eprintln!("  нужен StatusNotifierWatcher (в KDE штатный, в GNOME — расширение):");
            eprintln!("  sudo apt install gnome-shell-extension-appindicator");
            eprintln!("  gnome-extensions enable {}", APPINDICATOR_EXT);
            return 1;
        }
    };
    // состояние обновляем раз в 2 секунды: раскладку и сервис меняют извне
    let refresher = handle.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if refresher.update(|t: &mut PuntoTray| t.reload()).is_none() {
            return;
        }
    });
    worker(rx, handle, cfg_path);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_is_well_formed_argb() {
        for active in [true, false] {
            let icon = tray_icon(active);
            assert_eq!(icon.width, 22);
            assert_eq!(icon.height, 22);
            assert_eq!(icon.data.len(), 22 * 22 * 4, "ARGB32: 4 байта на пиксель");
            assert_eq!(&icon.data[0..4], &[0, 0, 0, 0], "угол прозрачный");
            let mid = ((11 * 22 + 11) * 4) as usize;
            assert_eq!(icon.data[mid], 255, "центр непрозрачный");
        }
    }

    #[test]
    fn stopped_icon_is_grey() {
        let icon = tray_icon(false);
        let mid = ((11 * 22 + 5) * 4) as usize;
        assert_eq!(
            icon.data[mid + 1],
            icon.data[mid + 2],
            "в выключенном состоянии половины одного цвета"
        );
    }

    #[test]
    fn auto_convert_toggle_edits_temp_config() {
        let dir = std::env::temp_dir().join(format!("punto-tray-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.ini");
        std::fs::write(&path, "[general]\nauto_convert = false ; комментарий\n").unwrap();
        assert_eq!(toggle_auto_convert(&path), Ok(true));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("auto_convert = true"), "{}", text);
        assert!(text.contains("; комментарий"), "комментарий сохранён");
        assert!(Config::load(&path).auto_convert);
        assert_eq!(toggle_auto_convert(&path), Ok(false));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_config_is_reported() {
        let path = PathBuf::from("/nonexistent/punto-rs/config.ini");
        let err = toggle_auto_convert(&path).unwrap_err();
        assert!(err.contains("нет"), "{}", err);
    }
}





