//! Punto-подобное переключение раскладки для Linux.
//!
//! Ядро (`dict`, `engine`, `buffer`, `keymap`) не зависит от платформы и
//! полностью тестируется. Ввод/вывод — отдельный слой: `io` собирается
//! только на Linux с фичей `input-evdev`.

pub mod buffer;
pub mod config;
pub mod dict;
pub mod doctor;
pub mod engine;
pub mod keymap;
pub mod layout;
pub mod output;
pub mod translit;
pub mod vkmap;

#[cfg(all(target_os = "linux", feature = "xkb"))]
pub mod keymap_xkb;

#[cfg(all(target_os = "linux", feature = "input-evdev"))]
pub mod io;

// иконка в трее: StatusNotifierItem по D-Bus, без GTK
#[cfg(all(target_os = "linux", feature = "tray"))]
pub mod tray;

/// Собранная «машина»: словари + движок + буфер.
pub mod app;
