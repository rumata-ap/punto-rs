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
            .name("punto-rs virtual keyboard")
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

/// Смена раскладки внешней командой (xkb-switch / setxkbmap / gsettings).
pub fn switch_layout(command: &str) -> io::Result<()> {
    let status = Command::new("sh").args(["-c", command]).status()?;
    if !status.success() {
        eprintln!("punto-rs: команда смены раскладки вернула {}", status);
    }
    Ok(())
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

fn keyboards() -> Vec<evdev::Device> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/dev/input") {
        for e in entries.flatten() {
            if !e.file_name().to_string_lossy().starts_with("event") {
                continue;
            }
            if let Ok(dev) = evdev::Device::open(e.path()) {
                let is_kbd = dev
                    .supported_keys()
                    .map(|k| {
                        k.contains(evdev::Key::KEY_A)
                            && k.contains(evdev::Key::KEY_SPACE)
                    })
                    .unwrap_or(false);
                if is_kbd {
                    out.push(dev);
                }
            }
        }
    }
    out
}

/// Главный цикл демона.
pub fn serve(mut app: App) -> io::Result<()> {
    let mut injector = Injector::new()?;
    let mut devs = keyboards();
    if devs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "нет клавиатур в /dev/input (пользователь должен быть в группе input)",
        ));
    }
    eprintln!("punto-rs: слушаю {} клавиатур(ы)", devs.len());
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

    loop {
        for dev in devs.iter_mut() {
            let events = match dev.fetch_events() {
                Ok(ev) => ev,
                Err(_) => continue,
            };
            for ev in events {
                if let evdev::InputEventKind::Key(key) = ev.kind() {
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
                    let action = if vk == app.cfg.convert_hotkey {
                        app.convert_now()
                    } else {
                        app.on_key(KeyPress { vk, shift, caps })
                    };
                    if let Action::Convert { to, erase, text } = action {
                        use crate::output::Strategy;
                        match &strategy {
                            Strategy::Uinput => {
                                let cmd = crate::layout::switch_command(&app.cfg, to);
                                switch_layout(&cmd)?;
                                injector.backspace(erase)?;
                                injector.type_text(&text)?;
                            }
                            Strategy::Clipboard => {
                                // текст идёт через буфер: раскладка не важна
                                injector.backspace(erase)?;
                                set_clipboard(&text)?;
                                injector.paste()?;
                                if switch_ok {
                                    let cmd = crate::layout::switch_command(&app.cfg, to);
                                    let _ = switch_layout(&cmd);
                                }
                            }
                            Strategy::Command(cmd) => {
                                injector.backspace(erase)?;
                                let quoted = text.replace('\'', "'\\''");
                                let full = cmd.replace("{text}", &format!("'{}'", quoted));
                                let _ = switch_layout(&full);
                                if switch_ok {
                                    let sc = crate::layout::switch_command(&app.cfg, to);
                                    let _ = switch_layout(&sc);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
