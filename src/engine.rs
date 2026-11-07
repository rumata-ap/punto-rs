//! Решение «переключать / не переключать» — перенос `sub_41C0F0`.
//!
//! Punto строит две гипотезы (текст как набран и тот же набор клавиш в другой
//! раскладке), сопоставляет обе со словарём и сравнивает признаки E/A/D.
//! Ниже сохранена исходная структура ветвлений, чтобы поведение совпадало.

use crate::dict::Dict;
use std::sync::Arc;

/// Куда идёт конвертация: через повторную проверку (`sub_41CAC0`)
/// или напрямую (`sub_41CC00` + `sub_41B410`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Route {
    /// не переключать
    None,
    /// конвертировать с повторной проверкой (`sub_41CAC0`)
    Recheck,
    /// конвертировать напрямую (`sub_41CC00`)
    Direct,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Decision {
    pub route: Route,
    /// VK-код последней клавиши на момент решения (для правила «минус»)
    pub last_vk: u16,
}

impl Decision {
    pub fn converts(&self) -> bool {
        self.route != Route::None
    }
}

pub struct Engine {
    /// словарь общий с ядром (`App`), чтобы не держать вторую копию: на полных
    /// словарях hunspell это сотни тысяч записей и десятки мегабайт
    pub dict: Arc<Dict>,
    /// код последней клавиши (`this+112` в Punto)
    pub last_vk: u16,
}

const VK_SUBTRACT: u16 = 0x6D;
const VK_OEM_MINUS: u16 = 0xBD;

impl Engine {
    pub fn new(dict: Arc<Dict>) -> Engine {
        Engine { dict, last_vk: 0 }
    }

    /// `typed` — текст в текущей раскладке, `converted` — тот же набор
    /// клавиш в другой раскладке.
    pub fn decide(&self, typed: &str, converted: &str) -> Decision {
        let cur = self.dict.match_word(typed);
        let oth = self.dict.match_word(converted);

        // v43: совпало слово с признаком D, а последняя клавиша — минус
        let special_de = cur.hit
            && cur.flags.d
            && (self.last_vk == VK_SUBTRACT || self.last_vk == VK_OEM_MINUS);

        // v44: «в другой раскладке» совпала user-запись с D+E
        let switch_de = oth.hit
            && oth.flags.d
            && oth.flags.e
            && (!cur.hit || !cur.flags.d || !cur.flags.e);

        let a = !cur.hit || cur.flags.e || special_de;
        if a && !switch_de {
            if cur.hit && cur.flags.e {
                return Decision { route: Route::None, last_vk: self.last_vk };
            }
            return Decision { route: Route::Recheck, last_vk: self.last_vk };
        }

        if cur.hit {
            if !cur.flags.e {
                if !cur.flags.a && !cur.flags.d {
                    if !oth.flags.a {
                        // v79 == HIBYTE(v78) || (BYTE)v77 != 0
                        if oth.hit == cur.flags.d || oth.flags.e {
                            return Decision { route: Route::Direct, last_vk: self.last_vk };
                        }
                        return Decision { route: Route::Recheck, last_vk: self.last_vk };
                    }
                    return Decision { route: Route::None, last_vk: self.last_vk };
                }
                return Decision { route: Route::Direct, last_vk: self.last_vk };
            }
        } else if !cur.flags.e {
            return Decision { route: Route::Direct, last_vk: self.last_vk };
        }

        Decision { route: Route::Direct, last_vk: self.last_vk }
    }
}
