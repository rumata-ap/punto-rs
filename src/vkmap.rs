//! Соответствие Windows VK ↔ Linux KEY_* (evdev).
//!
//! Нужно дважды: чтобы читать evdev и чтобы строить XKB-таблицу
//! (xkbcommon работает с кодами evdev + 8).

/// Linux KEY_* → Windows VK.
pub fn linux_to_vk(code: u16) -> Option<u16> {
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

/// Windows VK → Linux KEY_*.
pub fn vk_to_linux(vk: u16) -> Option<u16> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for vk in [0x41u16, 0x46, 0x51, 0x5A, 0xBA, 0xBD, 0x20] {
            let code = vk_to_linux(vk).unwrap();
            assert_eq!(linux_to_vk(code), Some(vk));
        }
        assert_eq!(vk_to_linux(0x46), Some(33)); // F → KEY_F
        assert_eq!(linux_to_vk(33), Some(0x46));
    }
}
