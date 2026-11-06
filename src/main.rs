//! CLI: `punto sim` (проверка алгоритма), `punto convert <текст>`, `punto run`.

use punto_rs::app::{Action, App};
use punto_rs::config::Config;
use punto_rs::keymap::{char_key, convert, KeyPress, Layout};
use std::io::{BufRead, Write};
use std::path::PathBuf;

fn config_path() -> PathBuf {
    match std::env::var_os("PUNTO_CONFIG") {
        Some(p) => PathBuf::from(p),
        None => {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            let base = std::env::var("XDG_CONFIG_HOME")
                .unwrap_or_else(|_| format!("{}/.config", home));
            PathBuf::from(format!("{}/punto-rs/config.ini", base))
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("sim");
    match mode {
        "sim" => sim(),
        "convert" => {
            let text = args[2..].join(" ");
            one_shot(&text);
        }
        "run" => run(),
        "-h" | "--help" | "help" => usage(),
        other => {
            eprintln!("неизвестный режим: {}", other);
            usage();
        }
    }
}

fn usage() {
    println!(
        "punto-rs — переключение раскладки как в Punto Switcher\n\n\
         uso:\n\
           punto sim        интерактивный прогон: строки на stdin, текущая раскладка RU\n\
           punto sim --en   то же, но текущая раскладка EN\n\
           punto convert X  показать решение для строки X\n\
           punto run        демон (Linux, сборка с --features input-evdev)\n\n\
         словари: ~/.config/punto-rs/*.dict (формат: <PBCEAD><TAB>текст)"
    );
}

fn load_app() -> App {
    let cfg = Config::load(&config_path());
    App::new(cfg)
}

fn sim() {
    let current = if std::env::args().any(|a| a == "--en") { Layout::En } else { Layout::Ru };
    let mut app = load_app();
    app.layout = current;
    println!("словарей записей: {}", app.dict.entries.len());
    println!("текущая раскладка: {:?}; вводите строки, пустая строка — выход", current);
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.is_empty() {
            break;
        }
        for ch in line.chars() {
            if let Some(k) = char_key(ch) {
                if let Action::Convert { to, erase, text } = app.on_key(k) {
                    report(to, erase, text, &app.last_reason);
                }
            }
        }
        // пробел завершает слово — как в Punto
        if let Action::Convert { to, erase, text } =
            app.on_key(KeyPress { vk: 0x20, shift: false, caps: false })
        {
            report(to, erase, text, &app.last_reason);
        }
        let _ = std::io::stdout().flush();
    }
}

fn report(to: Layout, erase: usize, text: String, reason: &str) {
    println!(
        "  -> переключить на {:?}: стереть {} и напечатать '{}'   [{}]",
        to, erase, text, reason
    );
}

fn one_shot(text: &str) {
    let app = load_app();
    // раскладку определяем по самому тексту: кириллица → RU, иначе EN
    let from = if text.chars().any(is_cyrillic) { Layout::Ru } else { Layout::En };
    let other = convert(text, from, from.other());
    let d = app.engine.decide(text, &other);
    println!("слово:        '{}'", text);
    println!("раскладка:     {:?} -> {:?}", from, from.other());
    println!("в др. раскл.:  '{}'", other);
    println!("совпадение A:  {:?}", app.dict.match_word(text));
    println!("совпадение B:  {:?}", app.dict.match_word(&other));
    println!(
        "решение:       {} ({:?})",
        if d.converts() { "переключать" } else { "не трогать" },
        d.route
    );
}

fn is_cyrillic(c: char) -> bool {
    ('\u{0400}'..='\u{04FF}').contains(&c)
}

#[cfg(all(target_os = "linux", feature = "input-evdev"))]
fn run() {
    let cfg = Config::load(&config_path());
    let app = App::new(cfg);
    match punto_rs::io::serve(app) {
        Ok(()) => {}
        Err(e) => eprintln!("ошибка ввода-вывода: {}", e),
    }
}

#[cfg(not(all(target_os = "linux", feature = "input-evdev")))]
fn run() {
    eprintln!(
        "режим run требует Linux и сборки с фичей input-evdev:\n  \
         cargo build --release --features input-evdev"
    );
}
