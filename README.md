# punto-rs — аналог Punto Switcher для Linux (Debian 13)

Автоматическое переключение раскладки: печатаете «ghbdtn» — получаете
«привет». Алгоритм восстановлен реверс-инжинирингом Punto Switcher 4.5.0.583
(см. `../punto-re/spec/algorithm.md`), данные — из открытых словарей.

## Состояние

| часть | статус |
|---|---|
| ядро (словарь, скорер, решение, буфер) | готово, 12 тестов зелёные |
| `punto sim` — прогон алгоритма без железа | работает (проверено) |
| `punto convert` — разбор одной строки | работает (проверено) |
| Linux-демон: evdev → uinput | написан, требует сборки на Debian |
| смена раскладки | внешняя команда (`xkb-switch -n` по умолчанию) |
| Wayland | через свою команду (`gsettings`/`swaymsg`), проверка на месте |
| `triggers.dat` (редкие исключения) | не переносили: влияет на единичные случаи |

Проверено на этой машине (`cargo test` → `12 passed`, сквозной прогон):

```
$ punto convert ghbdtn
слово:        'ghbdtn'
раскладка:     En -> Ru
в др. раскл.:  'привет'
совпадение A:  hit=true  e=false        (wrong.dict, без «валидно здесь»)
совпадение B:  hit=true  e=true         (ru.dict, настоящее слово)
решение:       переключать (Direct)

$ 'ghbdtn' | punto sim --en
  -> переключить на Ru: стереть 6 и напечатать 'привет'
```


## Сборка и проверка

```bash
# только ядро (работает на любой ОС)
cargo test

# демон для Debian 13
sudo apt install xkb-switch
cargo build --release --features input-evdev
```

## Использование

```bash
# 1. словари (из свободных hunspell-словарей)
sudo apt install hunspell-ru hunspell-en-us
python3 data/build_dict.py \
    --ru /usr/share/hunspell/ru_RU.dic \
    --en /usr/share/hunspell/en_US.dic \
    --out ~/.config/punto-rs

# 2. проверить решение на строке
./target/release/punto convert ghbdtn
#   слово:        'ghbdtn'
#   в др. раскл.: 'привет'
#   решение:      переключать (Direct)

# 3. интерактивно (строки на stdin, текущая раскладка — RU)
./target/release/punto sim

# 4. демон
./target/release/punto run
```

## Права для демона

```bash
# чтение клавиатуры
sudo usermod -aG input "$USER"          # перелогиниться

# запись в /dev/uinput
echo 'KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"' \
  | sudo tee /etc/udev/rules.d/99-punto-uinput.rules
sudo udevadm control --reload && sudo udevadm trigger
```

## Конфигурация

`~/.config/punto-rs/config.ini` (или путь из `PUNTO_CONFIG`):

```ini
[general]
max_word_len = 32
detect_passwords = true
dont_convert_capitals = false
convert_hotkey = 70            ; VK код ручной конвертации (70 = ScrollLock)
switch_command = xkb-switch -n ; wayland: gsettings ... / swaymsg ...
```

## Формат словарей

Одна запись на строку: `<ПРИЗНАКИ><TAB><текст>`.

| признаки | смысл |
|---|---|
| `PE` | настоящее слово языка: точное совпадение + «валидно здесь» |
| `B` | совпадение по префиксу, без признака «валидно здесь» |
| (пусто) | вхождение подстроки |
| `C` | учитывать регистр (`CapsLock`, `.net`, `A4Tech`) |

Три файла работают вместе:

* `ru.dict` — русские слова, `PE`;
* `en.dict` — английские слова, `PE`;
* `wrong.dict` — те же русские слова, **набранные в EN-раскладке**, `B`.

Именно `wrong.dict` заставляет алгоритм сработать: набранное «ghbdtn»
совпадает без признака «валидно», а «привет» — с признаком, поэтому
Punto-логика выбирает конвертацию.

## Как это соотносится с оригиналом

| Punto (Windows) | здесь |
|---|---|
| буфер нажатий (VK + Shift/Caps) | `src/buffer.rs` |
| `sub_423D10` клавиши→текст | `src/keymap.rs` |
| `sub_41F9C0` скорер (P/B/C + E/A/D) | `src/dict.rs` |
| `sub_41C0F0` решение | `src/engine.rs` |
| `FixTwoUpperLetters`, пароли, заглавные | `src/app.rs` |
| `sub_41B410` перепечатка | `src/io/mod.rs` (uinput) |

Отличия сознательные: вместо Win32-хука — evdev; вместо
`SendInput`/`IMM` — `uinput`; вместо хука раскладки — внешняя команда.

## Ограничения

* нумерация VK и Linux KEY-кодов сведена таблицей (`src/io/mod.rs`),
  нестандартные раскладки (Dvorak, макросы) не поддержаны;
* состояние раскладки отслеживается внутри процесса, внешняя смена
  раскладки пользователем (мышью в трее) не обнаруживается;
* демон не проверялся на живом Debian в этой сессии — только ядро
  (см. `cargo test`).

Лицензия: MIT OR Apache-2.0. Данные Яндекса не используются.

## Раскладка: X11, GNOME (Wayland), KDE

Демон определяет текущую раскладку и старается ставить нужную, а не просто
«следующую». Стратегии проверяются по очереди (`src/layout.rs`):

| среда | чтение | переключение |
|---|---|---|
| X11 | `xkb-switch -p` | `xkb-switch -s N` |
| GNOME, в т.ч. Wayland | `g3kb-switch -p` | `g3kb-switch -s N` |
| KDE Plasma (X11 и Wayland) | `qdbus org.kde.keyboard /Layouts …getLayout` | `…setLayout N` |
| GNOME без расширения | `gsettings get … mru-sources` | не гарантируется |

Свои команды — в конфиге:

```ini
layout_order       = en,ru
layout_get_command = g3kb-switch -p
layout_set_command = g3kb-switch -s {n}
```

Раз в 2 секунды демон перепроверяет раскладку, поэтому ручное переключение
(Alt+Shift, мышью) не рассинхронизирует состояние.

## Пакет и служба

```bash
make build          # бинарник с evdev/uinput
make test           # 21 тест
sh packaging/build-deb.sh    # build/punto-rs_0.1.0_amd64.deb
```

В пакет входят: `/usr/bin/punto-rs`, `punto-rs.service` (systemd --user),
udev-правило для `/dev/uinput`, словари в `/usr/share/punto-rs`.

```bash
sudo dpkg -i build/punto-rs_0.1.0_amd64.deb
sudo usermod -aG input "$USER"      # затем перелогиниться
systemctl --user enable --now punto-rs.service
```

## Wayland (GNOME, Ubuntu 24.04+ / 25.10+)

Первое, что нужно сделать на целевой машине:

```bash
punto doctor
```

Доктор скажет, что доступно, а чего не хватает, и какую стратегию выберет.

Проверенные факты, на которых построена работа под Wayland:

* **mutter не поддерживает `virtual-keyboard-unstable-v1`** (issue #1974
  открыт), поэтому `wtype` на GNOME Wayland **не работает**;
* **`uinput` работает везде** — это уровень ядра, композитор видит
  виртуальную клавиатуру как обычную;
* **буфер обмена + Ctrl+V через uinput** работает независимо от раскладки
  (нужен `wl-clipboard`);
* **смена раскладки** штатного CLI не имеет: на GNOME нужен
  `g3kb-switch` + расширение GNOME Shell; на KDE — D-Bus
  `org.kde.keyboard`; на sway/Hyprland — их собственные команды.

### Три стратегии вставки (`output` в конфиге)

| стратегия | как | когда выбирается |
|---|---|---|
| `uinput` | Backspace × N, затем печать кодами клавиш | есть рабочее переключение раскладки |
| `clipboard` | Backspace × N, `wl-copy`, Ctrl+V | переключения нет (GNOME без расширения) |
| `command` | Backspace × N, внешняя команда (`wtype {text}`) | wlroots-композиторы (sway, Hyprland) |

По умолчанию `output = auto`: wtype → uinput → clipboard.

### Полное поведение на GNOME Wayland (рекомендуется)

```bash
sudo apt install wl-clipboard libglib2.0-dev cmake
# собрать и установить g3kb-switch (CLI + расширение Shell)
# затем в конфиге:
#   layout_get_command = g3kb-switch -p
#   layout_set_command = g3kb-switch -s {n}
g3kb-switch -p     # проверка: печатает текущую группу
```
После установки расширения нужен перезаход в сессию.

С этим набором работает всё: слово перепечатывается **и** раскладка
переключается, как в Windows-Punto.

### Без расширения (деградация, но работает)

Ставим только `wl-clipboard`:

```bash
sudo apt install wl-clipboard
```

Демон сам выберет `clipboard`: текст подменяется через буфер, поэтому
**символы всегда правильные**, но раскладка не переключится — следующее
слово снова придётся исправлять. Для набора «одно слово не в той
раскладке» этого обычно достаточно.

### Ограничения буфера

* не работает в полях, где запрещена вставка (некоторые поля пароля);
* в терминалах вставка — `Ctrl+Shift+V`; для них задайте
  `type_command = xdotool type --clearmodifiers {text}` или используйте
  стратегию `uinput` с g3kb-switch.

### KDE и wlroots

* KDE Plasma (X11 и Wayland): `layout_get_command = qdbus org.kde.keyboard /Layouts org.kde.KeyboardLayouts.getLayout`, `layout_set_command = qdbus org.kde.keyboard /Layouts org.kde.KeyboardLayouts.setLayout {n}`;
* sway: `layout_set_command = swaymsg input type:keyboard xkb_switch_layout next`;
* Hyprland: `layout_set_command = hyprctl switchxkblayout all next`.


