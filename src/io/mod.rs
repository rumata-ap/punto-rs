//! Linux-слой: чтение клавиатуры через evdev, инъекция через uinput,
//! смена раскладки командой из конфигурации.
//!
//! Собирается только так:
//! ```text
//! cargo build --release --features input-evdev
//! ```
//! Требования на Debian 13: пользователь в группе `input` (чтение
//! `/dev/input/event*`), udev-правило на `/dev/uinput` (запись),
//! установленный `xkb-switch` (или своя команда в `switch_command`).
//!
//! ВНИМАНИЕ: этот модуль не проверялся на Windows-хосте (он собирается
//! только под Linux с фичей `input-evdev`); первый `cargo build --features
//! input-evdev` на Debian 13 нужно прогнать отдельно.

use crate::app::{Action, App};
use crate::keymap::{char_key, KeyPress};
use std::io;
use std::path::PathBuf;
use std::process::Command;

/// Linux KEY_* → Windows VK, чтобы использовать ту же таблицу, что и ядро.
fn key_to_vk(code: u16) -> Option<u16> {
    Some(match code {
        16 => 0x51, 17 => 0x57, 18 => 0x45, 19 => 0x52, 20 => 0x54, 21 => 0x59,
        22 => 0x55, 23 => 0x49, 24 => 0x4F, 25 => 0x50, 30 => 0x41, 31 => 0x53,
        32 => 0x44, 33 => 0x46, 34 => 0x47, 35 => 0x48, 36 => 0x4A, 37 => 0x4B,
        38 => 0x4C, 44 => 0x5A, 45 => 0x58, 46 => 0x43, 47 => 0x56, 48 => 0x42,
        49 => 0x4E, 50 => 0x4D, 39 => 0xBA, 13 => 0xBB, 51 => 0xBC, 12 => 0xBD,
        52 => 0xBE, 53 => 0xBF, 41 => 0xC0, 26 => 0xDB, 43 => 0xDC, 27 => 0xDD,
        57 => 0x20, 14 => 0x08, 28 => 0x0D, 15 => 0x09,
        _ => return None,
    })
}

/// Windows VK → Linux KEY_* для инъекции.
fn vk_to_key(vk: u16) -> Option<u16> {
    Some(match vk {
        0x51 => 16, 0x57 => 17, 0x45 => 18, 0x52 => 19, 0x54 => 20, 0x59 => 21,
        0x55 => 22, 0x49 => 23, 0x4F => 24, 0x50 => 25, 0x41 => 30, 0x53 => 31,
        0x44 => 32, 0x46 => 33, 0x47 => 34, 0x48 => 35, 0x4A => 36, 0x4B => 37,
        0x4C => 38, 0x5A => 44, 0x58 => 45, 0x43 => 46, 0x56 => 47, 0x42 => 48,
        0x4E => 49, 0x4D => 50, 0xBA => 39, 0xBB => 13, 0xBC => 51, 0xBD => 12,
        0xBE => 52, 0xBF => 53, 0xC0 => 41, 0xDB => 26, 0xDC => 43, 0xDD => 27,
        0x20 => 57, 0x08 => 14, 0x0D => 28,
        _ => return None,
    })
}


/// Имя виртуальной клавиатуры демона: её собственные события читать нельзя,
/// иначе Backspace/Ctrl+V возвращаются в буфер слова как новый ввод.
const VIRTUAL_KEYBOARD_NAME: &str = "punto-rs virtual keyboard";

/// Эмулятор клавиатуры через /dev/uinput.
pub struct Injector {
    dev: evdev::uinput::VirtualDevice,
}

impl Injector {
    pub fn new() -> io::Result<Injector> {
        let mut keys = evdev::AttributeSet::<evdev::Key>::new();
        let mut vks: Vec<u16> = (0x41..=0x5A).collect();
        vks.extend_from_slice(&[
            0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0, 0xDB, 0xDC, 0xDD,
            0x20, 0x08, 0x0D, 0x2A,
        ]);
        for vk in vks {
            if let Some(code) = vk_to_key(vk) {
                keys.insert(evdev::Key::new(code));
            }
        }
        keys.insert(evdev::Key::KEY_LEFTSHIFT);
        keys.insert(evdev::Key::KEY_LEFTCTRL);
        keys.insert(evdev::Key::KEY_V);
        let dev = evdev::uinput::VirtualDeviceBuilder::new()?
            .name(VIRTUAL_KEYBOARD_NAME)
            .with_keys(&keys)?
            .build()?;
        Ok(Injector { dev })
    }

    fn key_event(code: u16, value: i32) -> evdev::InputEvent {
        evdev::InputEvent::new(evdev::EventType::KEY, code, value)
    }

    pub fn tap(&mut self, vk: u16, shift: bool) -> io::Result<()> {
        let code = match vk_to_key(vk) {
            Some(c) => c,
            None => return Ok(()),
        };
        let shift_code = evdev::Key::KEY_LEFTSHIFT.code();
        if shift {
            self.dev.emit(&[Self::key_event(shift_code, 1)])?;
        }
        self.dev.emit(&[Self::key_event(code, 1), Self::key_event(code, 0)])?;
        if shift {
            self.dev.emit(&[Self::key_event(shift_code, 0)])?;
        }
        Ok(())
    }

    pub fn backspace(&mut self, n: usize) -> io::Result<()> {
        for _ in 0..n {
            self.tap(0x08, false)?;
        }
        Ok(())
    }

    /// Ctrl+V — вставка из буфера (работает независимо от раскладки).
    pub fn paste(&mut self) -> io::Result<()> {
        let ctrl = evdev::Key::KEY_LEFTCTRL.code();
        let v = evdev::Key::KEY_V.code();
        self.dev.emit(&[Self::key_event(ctrl, 1)])?;
        self.dev.emit(&[Self::key_event(v, 1), Self::key_event(v, 0)])?;
        self.dev.emit(&[Self::key_event(ctrl, 0)])?;
        Ok(())
    }

    pub fn type_text(&mut self, text: &str) -> io::Result<()> {
        for ch in text.chars() {
            if ch == ' ' {
                self.tap(0x20, false)?;
                continue;
            }
            if let Some(k) = char_key(ch) {
                self.tap(k.vk, k.shift)?;
            }
        }
        Ok(())
    }
}

/// Выполнить внешнюю команду через `sh -c` (печать текста, разовая смена
/// раскладки). Возвращает `false`, если команда не сработала.
pub fn run_shell(command: &str) -> bool {
    match Command::new("sh").args(["-c", command]).status() {
        Ok(status) if status.success() => true,
        Ok(status) => {
            eprintln!("punto-rs: команда '{}' вернула {}", command, status);
            false
        }
        Err(e) => {
            eprintln!("punto-rs: не удалось выполнить '{}': {}", command, e);
            false
        }
    }
}

/// Положить текст в буфер обмена (wl-copy). Нужен `wl-clipboard`.
pub fn set_clipboard(text: &str) -> io::Result<()> {
    let mut path = std::env::temp_dir();
    path.push(format!("punto-rs-clip-{}.txt", std::process::id()));
    std::fs::write(&path, text)?;
    let status = Command::new("sh")
        .args(["-c", &format!("wl-copy < '{}'", path.display())])
        .status()?;
    let _ = std::fs::remove_file(&path);
    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "wl-copy не сработал (поставьте wl-clipboard)",
        ));
    }
    Ok(())
}

/// Похоже ли устройство на клавиатуру: есть буквы и пробел.
fn is_keyboard(dev: &evdev::Device) -> bool {
    dev.supported_keys()
        .map(|k| k.contains(evdev::Key::KEY_A) && k.contains(evdev::Key::KEY_SPACE))
        .unwrap_or(false)
}

/// Пути клавиатур в `/dev/input`.
///
/// Устройства открываются только на время проверки: держать их здесь нельзя,
/// иначе поток чтения не смог бы открыть их повторно.
fn keyboard_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/dev/input") {
        for e in entries.flatten() {
            if !e.file_name().to_string_lossy().starts_with("event") {
                continue;
            }
            let path = e.path();
            if let Ok(dev) = evdev::Device::open(&path) {
                // свою виртуальную клавиатуру не слушаем: иначе демон
                // читает собственные Backspace/Ctrl+V и портит буфер слова
                if dev.name() == Some(VIRTUAL_KEYBOARD_NAME) {
                    continue;
                }
                if is_keyboard(&dev) {
                    out.push(path);
                }
            }
        }
    }
    out.sort();
    out
}

/// Уже открытые пути клавиатур (общий список потоков чтения и супервизора).
type Known = std::sync::Arc<std::sync::Mutex<std::collections::HashSet<PathBuf>>>;

/// Поток чтения одной клавиатуры. Переоткрывает устройство после ошибки.
///
/// Раньше любая ошибка чтения (`ENODEV` после USB-переподключения, сон,
/// смена устройства) молча завершала поток: демон оставался «живым», писал
/// «читаю N клавиатур(ы)», но клавиатуры больше не слышал. Снаружи это
/// выглядит ровно как «нажимаю Ctrl+Shift, и ничего не происходит».
fn spawn_listener(
    path: PathBuf,
    tx: std::sync::mpsc::Sender<evdev::InputEvent>,
    known: Known,
) -> bool {
    let registered = path.clone();
    let known_in_thread = std::sync::Arc::clone(&known);
    let spawned = std::thread::Builder::new()
        .name(String::from("punto-rs-input"))
        .spawn(move || {
            let mut misses = 0u32;
            loop {
                let mut dev = match evdev::Device::open(&path) {
                    Ok(d) => {
                        misses = 0;
                        d
                    }
                    Err(e) => {
                        misses += 1;
                        if misses == 1 {
                            eprintln!(
                                "punto-rs: {} не открывается ({}), пробую снова",
                                path.display(),
                                e
                            );
                        }
                        if misses > 10 {
                            // устройство ушло совсем: супервизор найдёт его заново
                            if let Ok(mut set) = known_in_thread.lock() {
                                set.remove(&path);
                            }
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                };
                loop {
                    match dev.fetch_events() {
                        Ok(events) => {
                            for ev in events {
                                if tx.send(ev).is_err() {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "punto-rs: {} — ошибка чтения ({}), переоткрываю",
                                path.display(),
                                e
                            );
                            break;
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        });
    if spawned.is_ok() {
        if let Ok(mut set) = known.lock() {
            set.insert(registered);
        }
        true
    } else {
        false
    }
}

/// Одна клавиша комбинации: подходит любой код из набора.
///
/// В конфиге `ctrl` (без стороны) означает «любой Ctrl», а `rctrl` — только
/// правый. Правые модификаторы есть не на каждой клавиатуре, и человек жмёт
/// ту сторону, которая под рукой.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChordKey {
    pub codes: Vec<u16>,
}

/// Комбинация ручной конвертации: две клавиши, для каждой — набор кодов.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub first: ChordKey,
    pub second: ChordKey,
}

impl Chord {
    /// 0 или 1 — к какой половине комбинации относится код.
    pub fn index_of(&self, code: u16) -> Option<usize> {
        if self.first.codes.contains(&code) {
            Some(0)
        } else if self.second.codes.contains(&code) {
            Some(1)
        } else {
            None
        }
    }

    /// По одному коду на клавишу — для инъекции (меню трея).
    pub fn inject(&self) -> (u16, u16) {
        (self.first.codes[0], self.second.codes[0])
    }
}

/// Разбор `convert_chord`: `ctrl+shift`, `rctrl+rshift`, `lalt+lshift`.
///
/// Имена без стороны (`ctrl`, `shift`, `alt`, `super`/`meta`) означают «любая
/// из двух клавиш».
pub fn parse_chord(spec: &str) -> Option<Chord> {
    let mut parts: Vec<ChordKey> = Vec::new();
    for part in spec.split('+') {
        let name = part.trim().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        let codes = match name.as_str() {
            "ctrl" => vec![
                evdev::Key::KEY_LEFTCTRL.code(),
                evdev::Key::KEY_RIGHTCTRL.code(),
            ],
            "lctrl" => vec![evdev::Key::KEY_LEFTCTRL.code()],
            "rctrl" => vec![evdev::Key::KEY_RIGHTCTRL.code()],
            "shift" => vec![
                evdev::Key::KEY_LEFTSHIFT.code(),
                evdev::Key::KEY_RIGHTSHIFT.code(),
            ],
            "lshift" => vec![evdev::Key::KEY_LEFTSHIFT.code()],
            "rshift" => vec![evdev::Key::KEY_RIGHTSHIFT.code()],
            "alt" => vec![
                evdev::Key::KEY_LEFTALT.code(),
                evdev::Key::KEY_RIGHTALT.code(),
            ],
            "lalt" => vec![evdev::Key::KEY_LEFTALT.code()],
            "ralt" => vec![evdev::Key::KEY_RIGHTALT.code()],
            "super" | "meta" => vec![
                evdev::Key::KEY_LEFTMETA.code(),
                evdev::Key::KEY_RIGHTMETA.code(),
            ],
            "lsuper" | "lmeta" => vec![evdev::Key::KEY_LEFTMETA.code()],
            "rsuper" | "rmeta" => vec![evdev::Key::KEY_RIGHTMETA.code()],
            _ => return None,
        };
        parts.push(ChordKey { codes });
    }
    match parts.as_slice() {
        // ровно две клавиши: одна клавиша — это уже не комбинация
        [a, b] => {
            // одна и та же клавиша с обеих сторон (`ctrl+lctrl`) дала бы
            // комбинацию, которая не сработает никогда
            if a.codes.iter().any(|c| b.codes.contains(c)) {
                return None;
            }
            Some(Chord { first: a.clone(), second: b.clone() })
        }
        _ => None,
    }
}

/// Буква или цифра в роли горячей клавиши.
///
/// Такую клавишу отбирать нельзя: она встречается при обычном наборе, и
/// конвертация начнёт срабатывать посреди слова (VK 70 — это «F», а не
/// ScrollLock; ScrollLock — 145).
fn hotkey_is_letter(vk: u16) -> bool {
    (0x30..=0x39).contains(&vk) || (0x41..=0x5A).contains(&vk)
}

/// Состояние комбинации ручной конвертации.
///
/// Срабатывание откладывается до отпускания обеих клавиш: иначе инъекция
/// (Backspace и символы) уйдёт в композитор с ещё зажатыми Ctrl+Shift и
/// превратится в Ctrl+Backspace / Ctrl+буква.
#[derive(Default)]
struct ChordState {
    down: [bool; 2],
    pending: bool,
}

impl ChordState {
    /// `index` — какая из двух клавиш комбинации, `pressed` — нажата/отпущена.
    /// Возвращает `true` в момент, когда комбинацию нажали и отпустили.
    fn update(&mut self, index: usize, pressed: bool) -> bool {
        let was_down = self.down[index];
        self.down[index] = pressed;
        if pressed && !was_down && self.down[1 - index] {
            self.pending = true;
        }
        if !pressed && self.pending && !self.down[0] && !self.down[1] {
            self.pending = false;
            return true;
        }
        false
    }

    /// Между клавишами комбинации нажали что-то ещё — это не конвертация.
    ///
    /// Без этого `Ctrl+Shift+V` (вставка в терминале) и `Ctrl+Shift+C`
    /// (копирование) считались бы комбинацией: на отпускании модификаторов
    /// демон стирал бы слово и перепечатывал его.
    fn cancel(&mut self) {
        self.pending = false;
    }
}

/// Открыть `/dev/uinput` с человеческим сообщением при отказе.
///
/// По умолчанию устройство принадлежит `root:root` с правами `0600`, поэтому
/// без udev-правила `Injector::new()` отдаёт голое `Permission denied`, и
/// единственным следом остаётся «ошибка ввода-вывода». Подсказываем лечение.
fn open_injector() -> io::Result<Injector> {
    Injector::new().map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => io::Error::new(
            io::ErrorKind::PermissionDenied,
            "нет прав на запись в /dev/uinput — без него нельзя ни стереть слово, \
             ни напечатать исправленное.\n  \
             постоянно: sudo cp packaging/99-punto-rs-uinput.rules /etc/udev/rules.d/ && \
             sudo udevadm control --reload && sudo udevadm trigger, затем перелогиниться\n  \
             разово:     sudo modprobe uinput && sudo chgrp input /dev/uinput && \
             sudo chmod 660 /dev/uinput",
        ),
        io::ErrorKind::NotFound => io::Error::new(
            io::ErrorKind::NotFound,
            "нет /dev/uinput: загрузите модуль (sudo modprobe uinput) и установите \
             udev-правило из packaging/",
        ),
        _ => e,
    })
}

/// Сверить раскладку с системой прямо сейчас.
///
/// Фоновый опрос идёт раз в 2 секунды, а ручную конвертацию жмут сразу после
/// набора: за эти две секунды раскладку могли переключить (или переключил сам
/// демон), и слово разобралось бы не в той. Стоит один вызов внешней команды на
/// нажатие комбинации — это дешевле, чем испорченное слово.
fn refresh_layout(app: &mut App) {
    if let Some(l) = crate::layout::detect(&app.cfg) {
        app.layout = l;
    }
}

/// Главный цикл демона.
pub fn serve(mut app: App) -> io::Result<()> {
    let mut injector = open_injector()?;
    let paths = keyboard_paths();
    if paths.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "нет клавиатур в /dev/input (пользователь должен быть в группе input)",
        ));
    }
    eprintln!("punto-rs: слушаю {} клавиатур(ы)", paths.len());
    let mut shift = false;
    let mut caps = false;
    if let Some(l) = crate::layout::detect(&app.cfg) {
        app.layout = l;
        eprintln!("punto-rs: текущая раскладка {:?}", l);
    } else {
        eprintln!("punto-rs: раскладку определить не удалось, считаю {:?}", app.layout);
    }
    let mut last_probe = std::time::Instant::now();
    let switch_ok = crate::layout::detect(&app.cfg).is_some();
    let wtype_ok = crate::doctor::has("wtype")
        && crate::doctor::session_kind() != "wayland";
    let strategy = crate::output::choose(&app.cfg, switch_ok, wtype_ok);
    eprintln!(
        "punto-rs: вывод текста — {} (переключение {}, wtype {})",
        strategy.name(),
        if switch_ok { "есть" } else { "нет" },
        if wtype_ok { "есть" } else { "нет" }
    );

    // `fetch_events` блокирующий (read(2) без O_NONBLOCK), поэтому каждое
    // устройство читается в своём потоке: при общем цикле демон «застревал»
    // на первой клавиатуре и не видел события с остальных.
    let (tx, rx) = std::sync::mpsc::channel::<evdev::InputEvent>();
    let known: Known = std::sync::Arc::new(std::sync::Mutex::new(
        std::collections::HashSet::new(),
    ));
    let mut listeners = 0usize;
    for path in paths {
        if spawn_listener(path, tx.clone(), std::sync::Arc::clone(&known)) {
            listeners += 1;
        }
    }
    // Клавиатура, подключённая после старта (USB, Bluetooth, док-станция),
    // раньше не замечалась вовсе: устройства открывались один раз. Демон при
    // этом оставался «живым», но новую клавиатуру не слышал.
    {
        let tx = tx.clone();
        let known = std::sync::Arc::clone(&known);
        std::thread::Builder::new()
            .name(String::from("punto-rs-rescan"))
            .spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(3));
                for path in keyboard_paths() {
                    let busy = known.lock().map(|s| s.contains(&path)).unwrap_or(true);
                    if !busy {
                        eprintln!("punto-rs: новая клавиатура — {}", path.display());
                        spawn_listener(path, tx.clone(), std::sync::Arc::clone(&known));
                    }
                }
            })
            .ok();
    }
    drop(tx);
    if listeners == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "не удалось запустить чтение клавиатур",
        ));
    }
    eprintln!("punto-rs: читаю {} клавиатур(ы)", listeners);

    // ручная конвертация комбинацией (например, Ctrl+Shift)
    let chord = parse_chord(&app.cfg.convert_chord);
    let mut chord_state = ChordState::default();
    match &chord {
        Some(_) => eprintln!("punto-rs: ручная конвертация — {}", app.cfg.convert_chord),
        None if app.cfg.convert_chord.trim().is_empty() => {
            eprintln!("punto-rs: ручная конвертация — выключена")
        }
        None => eprintln!(
            "punto-rs: ВНИМАНИЕ convert_chord = '{}' не разобран (нужны две клавиши: \
             ctrl, shift, alt, super — с указанием стороны или без) — \
             ручная конвертация выключена",
            app.cfg.convert_chord
        ),
    }

    // ручная конвертация отдельной клавишей (0 — выключена)
    let hotkey = app.cfg.convert_hotkey;
    if hotkey == 0 {
        eprintln!("punto-rs: горячая клавиша конвертации — выключена");
    } else if hotkey_is_letter(hotkey) {
        eprintln!(
            "punto-rs: ВНИМАНИЕ convert_hotkey = {} — это буква или цифра: \
             конвертация будет срабатывать при обычном наборе. Задайте 145 \
             (ScrollLock) или 0 (выключить)",
            hotkey
        );
    } else {
        eprintln!("punto-rs: горячая клавиша конвертации — VK {}", hotkey);
    }

    for ev in rx {
        let key = match ev.kind() {
            evdev::InputEventKind::Key(key) => key,
            _ => continue,
        };
        // раз в 2 секунды сверяем раскладку с системой:
        // пользователь мог переключить её сам
        if last_probe.elapsed() > std::time::Duration::from_secs(2) {
            last_probe = std::time::Instant::now();
            if let Some(l) = crate::layout::detect(&app.cfg) {
                app.layout = l;
            }
        }
        let code = key.code();
        let value = ev.value();
        // ручная конвертация комбинацией: ждём отпускания обеих клавиш
        if let Some(chord) = &chord {
            if let Some(index) = chord.index_of(code) {
                if chord_state.update(index, value != 0) {
                    // даём композитору снять модификаторы с состояния
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    refresh_layout(&mut app);
                    let action = app.convert_now();
                    // Без этой строки «ничего не происходит» невозможно
                    // отличить от «слово не понравилось» или «буфер пуст»:
                    // успешная конвертация раньше вообще не писала в журнал.
                    eprintln!("punto-rs: ручная конвертация: {}", app.last_reason);
                    apply_action(&mut app, &mut injector, &strategy, switch_ok, action)?;
                }
            } else if value != 0 {
                // третья клавиша между модификаторами — это не конвертация,
                // а Ctrl+Shift+V (вставка), Ctrl+Shift+C и подобные
                chord_state.cancel();
            }
        }
        if code == evdev::Key::KEY_LEFTSHIFT.code()
            || code == evdev::Key::KEY_RIGHTSHIFT.code()
        {
            shift = value != 0;
            continue;
        }
        if code == evdev::Key::KEY_CAPSLOCK.code() {
            if value == 1 {
                caps = !caps;
            }
            continue;
        }
        if value != 1 {
            continue;
        }
        let vk = match key_to_vk(code) {
            Some(v) => v,
            None => continue,
        };
        // горячая клавиша ручной конвертации; 0 — выключена.
        // Раньше дефолт был 70 = VK_F, и нажатие «f»/«а» посреди слова
        // запускало конвертацию: текст стирался, раскладка переключалась.
        let action = if hotkey != 0 && vk == hotkey {
            refresh_layout(&mut app);
            let action = app.convert_now();
            eprintln!("punto-rs: горячая клавиша VK {}: {}", hotkey, app.last_reason);
            action
        } else {
            app.on_key(KeyPress { vk, shift, caps })
        };
        apply_action(&mut app, &mut injector, &strategy, switch_ok, action)?;
    }
    Ok(())
}

/// Выполнить решение ядра: переключить раскладку, стереть слово, напечатать.
fn apply_action(
    app: &mut App,
    injector: &mut Injector,
    strategy: &crate::output::Strategy,
    switch_ok: bool,
    action: Action,
) -> io::Result<()> {
    use crate::output::Strategy;
    if let Action::Convert { to, erase, text } = action {
        match strategy {
            Strategy::Uinput => {
                // переключение с откатом: g3kb-switch -s N на GNOME 48 падает,
                // а -n работает (см. layout::switch_to)
                if !crate::layout::switch_to(&app.cfg, to) {
                    // раскладку переключить не удалось: перепечатывать в старой
                    // бессмысленно — получился бы тот же текст
                    eprintln!(
                        "punto-rs: раскладку на {:?} переключить не удалось — слово не тронуто",
                        to
                    );
                    // слово на экране осталось прежним — циклу «туда-обратно»
                    // нечего возвращать
                    app.last_word = None;
                    return Ok(());
                }
                // раскладку применяет композитор, это не мгновенно: без паузы
                // символы успевают уйти в старую раскладку
                std::thread::sleep(std::time::Duration::from_millis(80));
                injector.backspace(erase)?;
                injector.type_text(&text)?;
            }
            Strategy::Clipboard => {
                // текст идёт через буфер: раскладка не важна
                injector.backspace(erase)?;
                set_clipboard(&text)?;
                injector.paste()?;
                if switch_ok {
                    crate::layout::switch_to(&app.cfg, to);
                }
            }
            Strategy::Command(cmd) => {
                injector.backspace(erase)?;
                let quoted = text.replace('\'', "'\\''");
                let full = cmd.replace("{text}", &format!("'{}'", quoted));
                run_shell(&full);
                if switch_ok {
                    crate::layout::switch_to(&app.cfg, to);
                }
            }
        }
        // Если раскладку на самом деле не переключили (clipboard/command),
        // внутреннее состояние разъехалось бы с системным до следующей
        // проверки раз в 2 секунды — и следующее слово разобралось бы не в
        // той раскладке. Сверяемся сразу после конвертации.
        if let Some(l) = crate::layout::detect(&app.cfg) {
            app.layout = l;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_hotkeys_are_flagged() {
        // VK 70 = «F»: именно из-за него конвертация срабатывала при наборе
        assert!(hotkey_is_letter(70), "VK_F — буква");
        assert!(hotkey_is_letter(0x41));
        assert!(hotkey_is_letter(0x30), "цифры тоже нельзя");
        assert!(!hotkey_is_letter(0), "0 — выключено, это не буква");
        assert!(!hotkey_is_letter(0x91), "ScrollLock (145) допустим");
        assert!(!hotkey_is_letter(0x13), "Pause (19) допустим");
    }

    #[test]
    fn chord_parsing() {
        let right = parse_chord("rctrl+rshift").expect("rctrl+rshift");
        assert_eq!(right.first.codes, vec![evdev::Key::KEY_RIGHTCTRL.code()]);
        assert_eq!(right.second.codes, vec![evdev::Key::KEY_RIGHTSHIFT.code()]);
        assert_eq!(
            parse_chord(" RCTRL + RSHIFT "),
            Some(right),
            "регистр и пробелы не важны"
        );

        // имя без стороны — годится любая из двух клавиш
        let any = parse_chord("ctrl+shift").expect("ctrl+shift");
        assert_eq!(any.first.codes.len(), 2);
        assert_eq!(any.index_of(evdev::Key::KEY_LEFTCTRL.code()), Some(0));
        assert_eq!(any.index_of(evdev::Key::KEY_RIGHTCTRL.code()), Some(0));
        assert_eq!(any.index_of(evdev::Key::KEY_LEFTSHIFT.code()), Some(1));
        assert_eq!(any.index_of(evdev::Key::KEY_RIGHTSHIFT.code()), Some(1));
        assert_eq!(any.index_of(evdev::Key::KEY_A.code()), None);
        // для инъекции трея берётся по одному коду
        assert_eq!(
            any.inject(),
            (
                evdev::Key::KEY_LEFTCTRL.code(),
                evdev::Key::KEY_LEFTSHIFT.code()
            )
        );

        assert_eq!(parse_chord(""), None);
        assert_eq!(parse_chord("rctrl"), None, "нужны ровно две клавиши");
        assert_eq!(parse_chord("rctrl+rshift+ralt"), None);
        assert_eq!(parse_chord("rctrl+нетакой"), None);
        assert_eq!(parse_chord("ctrl+lctrl"), None, "одна клавиша дважды");
        assert!(parse_chord("lalt+lshift").is_some());
    }

    #[test]
    fn chord_fires_only_after_all_keys_released() {
        let mut s = ChordState::default();
        // нажали правый Ctrl — ничего
        assert!(!s.update(0, true));
        // нажали правый Shift при зажатом Ctrl — тоже ничего: инъекция должна
        // ждать, пока модификаторы отпустят
        assert!(!s.update(1, true));
        // отпустили Shift, Ctrl ещё зажат — ждём
        assert!(!s.update(1, false));
        // отпустили Ctrl — теперь можно конвертировать
        assert!(s.update(0, false));
        // повторных срабатываний нет
        assert!(!s.update(0, false));
    }

    #[test]
    fn chord_order_does_not_matter_and_repeat_is_ignored() {
        let mut s = ChordState::default();
        assert!(!s.update(1, true));
        assert!(!s.update(1, true)); // автоповтор
        assert!(!s.update(0, true));
        assert!(s.update(1, false) || s.update(0, false));
    }

    /// Ctrl+Shift+V в терминале — это вставка, а не конвертация: третья клавиша
    /// между модификаторами отменяет комбинацию.
    #[test]
    fn third_key_cancels_chord() {
        let mut s = ChordState::default();
        assert!(!s.update(0, true));
        assert!(!s.update(1, true));
        s.cancel(); // нажали V
        assert!(!s.update(1, false));
        assert!(
            !s.update(0, false),
            "после третьей клавиши комбинация не срабатывает"
        );
        // следующая «чистая» комбинация снова работает
        assert!(!s.update(0, true));
        assert!(!s.update(1, true));
        assert!(!s.update(0, false));
        assert!(s.update(1, false));
    }
}
