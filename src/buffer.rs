//! Буфер текущего слова: то, что в Punto хранится как массив нажатий
//! (объект `CPuntoWnd`, поле со смещением `+0x70` — код последней клавиши).

use crate::keymap::{text_for, KeyPress, Layout};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Push {
    /// символ вошёл в слово
    Word,
    /// разделитель: слово закончилось
    Boundary,
}

#[derive(Default)]
pub struct WordBuffer {
    keys: Vec<KeyPress>,
    max_len: usize,
    pub last_vk: u16,
}

impl WordBuffer {
    pub fn new(max_len: usize) -> WordBuffer {
        WordBuffer { keys: Vec::new(), max_len, last_vk: 0 }
    }

    pub fn push(&mut self, k: KeyPress, word_chars: &str) -> Push {
        self.last_vk = k.vk;
        let ch = crate::keymap::key_char(k, Layout::En)
            .or_else(|| crate::keymap::key_char(k, Layout::Ru));
        match ch {
            Some(c) if word_chars.contains(c) || c.is_alphanumeric() => {
                if self.keys.len() >= self.max_len {
                    self.keys.remove(0);
                }
                self.keys.push(k);
                Push::Word
            }
            _ => Push::Boundary,
        }
    }

    pub fn clear(&mut self) {
        self.keys.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn keys(&self) -> &[KeyPress] {
        &self.keys
    }

    /// Текст слова в указанной раскладке.
    pub fn text(&self, layout: Layout) -> String {
        text_for(&self.keys, layout)
    }

    /// Убрать последнее нажатие (Backspace).
    pub fn pop(&mut self) -> Option<KeyPress> {
        self.keys.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(vk: u16) -> KeyPress {
        KeyPress { vk, shift: false, caps: false }
    }

    #[test]
    fn word_and_boundary() {
        let mut b = WordBuffer::new(32);
        for vk in [0x47, 0x48, 0x42, 0x44, 0x54, 0x4E] {
            assert_eq!(b.push(k(vk), ""), Push::Word);
        }
        assert_eq!(b.text(Layout::Ru), "привет");
        assert_eq!(b.text(Layout::En), "ghbdtn");
        // разделитель только сообщает о границе; слово остаётся,
        // чтобы вызывающий код успел принять решение
        assert_eq!(b.push(k(0x20), ""), Push::Boundary);
        assert_eq!(b.len(), 6);
        b.clear();
        assert!(b.is_empty());
    }
}
