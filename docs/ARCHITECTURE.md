# Архитектура punto-rs

## Поток данных

```
/dev/input/event*            (evdev, Linux)
   │ EV_KEY press
   ▼
buffer::WordBuffer           буфер нажатий: VK + Shift + Caps
   │
   ├── keymap::text_for(keys, текущая)   → «как набрано»
   └── keymap::text_for(keys, другая)    → «тот же набор клавиш иначе»
   │
   ▼
dict::Dict::match_word(x2)   скорер: точное / префикс / подстрока + E/A/D
   │
   ▼
engine::Engine::decide       перенос sub_41C0F0
   │ Route::None / Recheck / Direct
   ▼
app::App::evaluate           правила: заглавные, пароли, нет букв
   │ Action::Convert { to, erase, text }
   ▼
io::switch_layout + uinput   смена раскладки, Backspace×N, печать текста
```

## Модули

| файл | ответственность | зависимость от ОС |
|---|---|---|
| `keymap.rs` | таблица «клавиша → символ» для US и RU, конвертация строки | нет |
| `buffer.rs` | буфер слова, разделители, Backspace | нет |
| `dict.rs` | формат словаря, режимы сопоставления, признаки | нет |
| `engine.rs` | решение по двум гипотезам | нет |
| `app.rs` | связка + правила верхнего уровня | нет |
| `translit.rs` | транслитерация имён | нет |
| `config.rs` | ini-конфиг | нет |
| `io/mod.rs` | evdev-ввод, uinput-вывод, смена раскладки | Linux + feature |

Ядро (всё до `io`) не знает про ОС и покрыто тестами в `tests/engine_tests.rs`.

## Почему именно так

Punto хранит **буфер нажатий**, а не текст: одна и та же
последовательность клавиш даёт два текста — в текущей раскладке и в другой.
Это ключ к алгоритму: решение принимается сравнением двух гипотез по
словарю, а не эвристикой по одной строке.

Признак `E` («валидное слово языка») — то, что отличает настоящий текст от
мусора:

* «привет» — есть в `ru.dict` с `E`;
* «ghbdtn» — есть в `wrong.dict`, но без `E`.

Поэтому «ghbdtn» проигрывает конкуренцию «привет» и слово перепечатывается.

## Смена раскладки

Punto патчит функции раскладки (`layouts.exe`, `hook.patch_layout_funcs`).
На Linux эквивалент — внешняя команда:

| среда | команда |
|---|---|
| X11 (любой WM) | `xkb-switch -n` |
| X11 без xkb-switch | `setxkbmap -layout us,ru -option grp:alt_shift_toggle` + своя логика |
| GNOME Wayland | `gsettings set org.gnome.desktop.input-sources current N` |
| sway | `swaymsg input type:keyboard xkb_switch_layout next` |

Команда задаётся в конфиге, поэтому демон не привязан к одному окружению.

## Вставка текста (Wayland)

`io`: после решения нужно стереть слово и напечатать новое. Способ зависит
от среды (`src/output.rs`):

| стратегия | механика | ограничение |
|---|---|---|
| `uinput` | коды клавиш через `/dev/uinput` | символы зависят от текущей раскладки |
| `clipboard` | `wl-copy` + Ctrl+V через uinput | не везде разрешена вставка |
| `command` | `wtype {text}` и подобные | работает только на wlroots |

Порядок выбора в `auto`: `command` (если wtype) → `uinput` (если есть
переключение) → `clipboard` (всегда возможен при `wl-clipboard`).

`punto doctor` (`src/doctor.rs`) — диагностика: сессия, доступ к
`/dev/input` и `/dev/uinput`, наличие инструментов, выбранная стратегия.

## Что дальше

1. Читать текущую раскладку из окружения, а не хранить в процессе.
2. Поддержка `xkbcommon` для нестандартных раскладок (фича `xkb`).
3. Перенос `triggers.dat`-логики, если понадобится.
4. Юнит `systemd --user` + `.deb` (debhelper).
