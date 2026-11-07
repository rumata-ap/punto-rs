//! Тесты ядра: перенос поведения Punto (решение, режимы сопоставления).

use punto_rs::app::{fix_two_upper_letters, looks_like_password};
use punto_rs::dict::{flags_from_letters, Dict, Flags, F_CASE, F_E, F_EXACT, F_PREFIX};
use punto_rs::engine::{Engine, Route};
use punto_rs::keymap::{convert, KeyPress, Layout};
use std::sync::Arc;

const CHARSET: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ\
абвгдеёжзийклмнопрстуфхцчшщъыьэюя\
АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ\
0123456789-_";

fn dict() -> Dict {
    let mut d = Dict::new(CHARSET);
    // настоящие слова обоих языков: точное совпадение + признак E
    for w in ["привет", "пока", "текст", "слово"] {
        d.add(w, F_EXACT | F_E);
    }
    for w in ["hello", "default"] {
        d.add(w, F_EXACT | F_E);
    }
    // «мусор»: русские слова, набранные в EN-раскладке — без E
    for w in ["ghbdtn", "gjrf", "ntrcn"] {
        d.add(w, F_PREFIX);
    }
    d
}

#[test]
fn garbage_latin_switches_to_russian() {
    let e = Engine::new(Arc::new(dict()));
    let d = e.decide("ghbdtn", "привет");
    assert!(d.converts(), "ghbdtn -> привет должно переключать");
    assert_eq!(d.route, Route::Direct);
}

#[test]
fn real_english_is_kept() {
    let e = Engine::new(Arc::new(dict()));
    let d = e.decide("hello", "руддщ");
    assert!(!d.converts(), "настоящее английское слово трогать нельзя");
}

#[test]
fn real_russian_is_kept() {
    let e = Engine::new(Arc::new(dict()));
    let d = e.decide("привет", "ghbdtn");
    assert!(!d.converts(), "настоящее русское слово трогать нельзя");
}

#[test]
fn key_roundtrip() {
    assert_eq!(convert("привет", Layout::Ru, Layout::En), "ghbdtn");
    assert_eq!(convert("ghbdtn", Layout::En, Layout::Ru), "привет");
    assert_eq!(convert("ntrcn", Layout::En, Layout::Ru), "текст");
}

#[test]
fn buffer_keys_give_both_hypotheses() {
    use punto_rs::buffer::WordBuffer;
    let mut b = WordBuffer::new(32);
    for vk in [0x47u16, 0x48, 0x42, 0x44, 0x54, 0x4E] {
        b.push(KeyPress { vk, shift: false, caps: false }, CHARSET);
    }
    assert_eq!(b.text(Layout::Ru), "привет");
    assert_eq!(b.text(Layout::En), "ghbdtn");
}

#[test]
fn dict_match_modes() {
    let mut d = Dict::new(CHARSET);
    d.add("abc", F_EXACT);
    d.add("xy", F_PREFIX);
    d.add("qq", 0);
    assert!(d.match_word("abc").hit);
    assert!(!d.match_word("abcd").hit, "точное не совпадает с более длинным");
    assert!(d.match_word("xyzzz").hit, "префикс совпадает");
    assert!(d.match_word("zzqqzz").hit, "подстрока по умолчанию");
    assert!(!d.match_word("nope").hit);
}

#[test]
fn case_flag_is_respected() {
    let mut d = Dict::new(CHARSET);
    d.add("CapsLock", F_EXACT | F_CASE);
    assert!(d.match_word("CapsLock").hit);
    assert!(!d.match_word("capslock").hit);
    let mut d2 = Dict::new(CHARSET);
    d2.add("hello", F_EXACT);
    assert!(d2.match_word("HELLO").hit, "без флага C регистр не важен");
}

#[test]
fn flags_mapping() {
    let f = flags_from_letters("PE");
    assert_eq!(f & F_EXACT, F_EXACT);
    assert_eq!(f & F_E, F_E);
    assert_eq!(f & F_PREFIX, 0);

    let f2 = flags_from_letters("B");
    assert_eq!(f2 & F_PREFIX, F_PREFIX);
    assert_eq!(f2 & F_E, 0);

    let f3 = Flags::from_bits(flags_from_letters("P"));
    assert!(!f3.e && !f3.d);
}

#[test]
fn word_trimming() {
    let mut d = Dict::new("abcdefghijklmnopqrstuvwxyz");
    d.add("ghbdtn", F_EXACT | F_E);
    // Punto обрезает строку до хвоста из символов набора
    assert!(d.match_word("...ghbdtn").hit);
    assert_eq!(d.trim_word("...ghbdtn"), "ghbdtn");
}

#[test]
fn recheck_refuses_unknown_target() {
    use punto_rs::app::should_recheck_convert;
    let mut d = Dict::new(CHARSET);
    d.add("привет", F_EXACT | F_E);
    let e = Engine::new(Arc::new(d.clone()));
    // ничего не совпало ни в одной гипотезе → движок отдаёт Recheck
    let dec = e.decide("компь", "rjvgm");
    assert_eq!(dec.route, Route::Recheck);
    // повторный контроль (аналог sub_41CAC0) не даёт печатать «мусор в мусор»
    assert!(!should_recheck_convert(&d, "rjvgm"));
    assert!(should_recheck_convert(&d, "привет"));
}

#[test]
fn fix_two_upper_letters_rule() {
    assert_eq!(fix_two_upper_letters("ПРИВЕТ"), "Привет");
    assert_eq!(fix_two_upper_letters("Привет"), "Привет");
}

#[test]
fn password_heuristic() {
    assert!(looks_like_password("/ha"));
    assert!(looks_like_password("ab12!@#cd"));
    assert!(!looks_like_password("привет"));
    assert!(!looks_like_password("hello"));
}
