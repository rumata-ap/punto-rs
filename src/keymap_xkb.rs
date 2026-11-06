//! Раскладко-независимое преобразование через libxkbcommon (фича `xkb`).
//!
//! Punto берёт символы у Win32 (`ToUnicodeEx`/`MapVirtualKey`). На Linux
//! правильный эквивалент — xkbcommon: он знает все раскладки системы,
//! включая нестандартные (Dvorak, макросы, несколько групп).
//!
//! Модуль собирается только под Linux с фичей `xkb`:
//! ```text
//! cargo build --release --features "input-evdev xkb"
//! ```
//!
//! Таблица «код клавиши → символ» строится один раз при старте: для каждой
//! группы раскладки и для двух уровней (обычный/Shift) перебираются коды.

use crate::keymap::{KeyPress, Layout};
use std::collections::HashMap;
use xkbcommon::xkb;

pub struct XkbMapper {
    /// (группа, shift) → код клавиши → символ
    tables: Vec<HashMap<u16, char>>,
    layout_group: [usize; 2],
}

fn build_table(state: &mut xkb::State, group: u32, shift: bool) -> HashMap<u16, char> {
    let mut mods = xkb::ModMask::empty();
    if shift {
        mods |= xkb::ModMask::SHIFT;
    }
    state.update_mask(mods, mods, 0, 0, 0, 0);
    state.update_mask(0, xkb::ModMask::empty(), group, 0, 0, 0);
    let mut map = HashMap::new();
    for code in 8u16..248 {
        let sym = state.key_get_one_sym(xkb::Keycode::new(code as u32));
        let name = xkb::keysym_get_name(sym);
        if name.is_empty() {
            continue;
        }
        let text = sym_to_char(&name);
        if let Some(c) = text {
            map.insert(code, c);
            // второй уровень (Shift) отдаём под тем же кодом отдельно
            if shift {
                continue;
            }
        }
    }
    map
}

/// keysym → символ (простое покрытие латиницы/кириллицы/цифр/пунктуации).
fn sym_to_char(name: &str) -> Option<char> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Some(c);
    }
    // Uxxxx — юникод напрямую
    if let Some(hex) = name.strip_prefix('U') {
        if let Ok(v) = u32::from_str_radix(hex, 16) {
            return char::from_u32(v);
        }
    }
    None
}

impl XkbMapper {
    /// `layouts` — имена через запятую, как в системе (`us,ru`).
    pub fn new(layouts: &str) -> Option<XkbMapper> {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_DEFAULT_INCLUDES);
        let keymap = xkb::Keymap::new_from_names(
            &ctx,
            "",
            "",
            "",
            "",
            Some(layouts.to_string()),
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )?;
        let mut state = xkb::State::new(&keymap);
        let mut tables = Vec::new();
        let mut layout_group = [0usize, 0usize];
        // две группы и два уровня
        for group in 0..2u32 {
            for shift in [false, true] {
                tables.push(build_table(&mut state, group, shift));
            }
        }
        // предполагаем порядок «en,ru», как в layout_order
        layout_group[0] = 0; // En
        layout_group[1] = 1; // Ru
        Some(XkbMapper { tables, layout_group })
    }

    pub fn text_for(&self, keys: &[KeyPress], layout: Layout) -> String {
        let group = match layout {
            Layout::En => self.layout_group[0],
            Layout::Ru => self.layout_group[1],
        };
        let mut out = String::new();
        for k in keys {
            let code = match crate::vkmap::vk_to_linux(k.vk) {
                Some(c) => c,
                None => continue,
            };
            let level = usize::from(k.shift);
            let table = &self.tables[group * 2 + level];
            match table.get(&code) {
                Some(c) => {
                    if (k.shift ^ k.caps) && c.is_alphabetic() {
                        out.extend(c.to_uppercase());
                    } else {
                        out.push(*c);
                    }
                }
                None => {}
            }
        }
        out
    }
}
