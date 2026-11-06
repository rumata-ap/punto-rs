//! Раскладко-независимое преобразование через libxkbcommon (фича `xkb`).
//!
//! Punto берёт символы у Win32 (`ToUnicodeEx`/`MapVirtualKey`). На Linux
//! правильный эквивалент — xkbcommon: он знает все раскладки системы,
//! включая нестандартные (Dvorak, несколько групп, макросы).
//!
//! Собирается только под Linux с фичей `xkb`:
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
    /// (группа * 2 + shift) → код клавиши → символ
    tables: Vec<HashMap<u16, char>>,
    /// индекс группы для En и Ru в системе
    layout_group: [usize; 2],
}

/// evdev-код левого Shift (xkbcommon принимает коды с +8).
const KEY_LEFTSHIFT_CODE: u32 = 42;

fn build_table(state: &mut xkb::State, group: u32, shift: bool) -> HashMap<u16, char> {
    state.update_mask(0, 0, 0, group, 0, 0);
    let shift_key = xkb::Keycode::new(KEY_LEFTSHIFT_CODE);
    let mut map = HashMap::new();
    for code in 8u16..248 {
        let kc = xkb::Keycode::new(code as u32);
        if shift {
            state.update_key(shift_key, xkb::KeyDirection::Down);
        }
        let sym = state.key_get_one_sym(kc);
        if shift {
            state.update_key(shift_key, xkb::KeyDirection::Up);
        }
        if let Some(c) = char::from_u32(xkb::keysym_to_utf32(sym)) {
            if !c.is_control() {
                map.insert(code, c);
            }
        }
    }
    map
}

impl XkbMapper {
    /// `layouts` — имена раскладок через запятую, как в системе (`us,ru`).
    pub fn new(layouts: &str) -> Option<XkbMapper> {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_DEFAULT_INCLUDES);
        let keymap = xkb::Keymap::new_from_names::<str>(
            &ctx,
            "",
            "",
            layouts,
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )?;
        let mut state = xkb::State::new(&keymap);
        let mut tables = Vec::new();
        for group in 0..2u32 {
            for shift in [false, true] {
                tables.push(build_table(&mut state, group, shift));
            }
        }
        Some(XkbMapper { tables, layout_group: [0, 1] })
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
            if let Some(c) = self.tables[group * 2 + level].get(&code) {
                if (k.shift ^ k.caps) && c.is_alphabetic() {
                    out.extend(c.to_uppercase());
                } else {
                    out.push(*c);
                }
            }
        }
        out
    }
}
