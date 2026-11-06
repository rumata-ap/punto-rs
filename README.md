# punto-rs — аналог Punto Switcher для Linux (Debian 13 / Ubuntu, X11 и Wayland)

Печатаете «ghbdtn» — получаете «привет»: слово набрано не в той раскладке,
и программа исправляет его сама. Алгоритм восстановлен реверс-инжинирингом
Punto Switcher 4.5.0.583 (см. `../punto-re/spec/algorithm.md`), словари —
из открытых источников.

## Состояние

| часть | статус |
|---|---|
| ядро: словарь, скорер, решение, буфер | готово, **23 теста** |
| `punto convert` / `punto sim` / `punto doctor` | работают, проверено |
| Linux-слой: evdev → uinput | **собрано и проверено на Linux** (Ubuntu 24.04) |
| режим `xkbcommon` (`--features xkb`) | собирается на Linux |
| смена раскладки | `xkb-switch` / `g3kb-switch` / KDE D-Bus / sway / Hyprland |
| Wayland | три стратегии вставки, полное поведение — с `g3kb-switch` |
| `triggers.dat` оригинала | не перенесён (влияет на единичные случаи) |

Что именно проверено на реальном Linux (Ubuntu 24.04, Wayland-сессия в WSL):

```
cargo build  --features input-evdev        → Finished (7.1 s)
cargo check  --features "input-evdev xkb"  → Finished (3.8 s)
cargo test                                 → 12 passed, 0 failed
./target/debug/punto doctor                → отработал и верно диагностировал среду
```

Сквозной прогон ядра (на любой ОС):

```
$ punto convert ghbdtn
слово:        'ghbdtn'
раскладка:     En -> Ru
в др. раскл.:  'привет'
совпадение A:  hit=true  e=false      (wrong.dict — «набрано не в той раскладке»)
совпадение B:  hit=true  e=true       (ru.dict — настоящее слово)
решение:       переключать (Direct)
```

## Быстрый старт (Debian 13 / Ubuntu)

```bash
sudo apt install build-essential pkg-config wl-clipboard hunspell-ru hunspell-en-us
# X11:
sudo apt install xkb-switch
# сборка
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # если нет rust
git clone https://github.com/rumata-ap/punto-rs && cd punto-rs
cargo build --release --features input-evdev

# словари из свободных hunspell-словарей
python3 data/build_dict.py \
    --ru /usr/share/hunspell/ru_RU.dic \
    --en /usr/share/hunspell/en_US.dic \
    --out ~/.config/punto-rs

# права и проверка среды
sudo usermod -aG input "$USER"        # затем перелогиниться
./target/release/punto doctor         # покажет, чего не хватает и что выбрано

# запуск
./target/release/punto run
```

## Зависимости по средам

| что | зачем | обязательность |
|---|---|---|
| группа `input` | чтение `/dev/input/event*` | **обязательно** |
| `/dev/uinput` + udev-правило | эмуляция клавиатуры (Backspace, Ctrl+V) | **обязательно** |
| `wl-clipboard` | стратегия `clipboard` (Wayland без расширения) | рекомендуется |
| `g3kb-switch` + расширение GNOME | переключение раскладки на GNOME (и Wayland) | для полного поведения |
| `xkb-switch` | переключение и чтение раскладки на X11 | для X11 |
| `qdbus` (`qdbus-qt5`) | KDE: `org.kde.keyboard` | для KDE |
| `hunspell-ru`, `hunspell-en-us` | словари | рекомендуется |
| `libxkbcommon-dev`, `pkg-config` | сборка с `--features xkb` | по желанию |

## Права для демона

```bash
# чтение клавиатуры
sudo usermod -aG input "$USER"          # перелогиниться

# запись в /dev/uinput (или sudo modprobe uinput + правило из packaging/)
sudo cp packaging/99-punto-rs-uinput.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

## Конфигурация

`~/.config/punto-rs/config.ini` (путь переопределяется `PUNTO_CONFIG`).
Полный пример — `data/config.example.ini`. Ключевые параметры:

| ключ | смысл | по умолчанию |
|---|---|---|
| `max_word_len` | максимум символов в слове | `32` |
| `detect_passwords` | не трогать похожее на пароль | `true` |
| `dont_convert_capitals` | не трогать слова с заглавной | `false` |
| `convert_hotkey` | VK-код ручной конвертации (`70` = ScrollLock, `19` = Pause) | `70` |
| `switch_command` | «следующая раскладка» (X11) | `xkb-switch -n` |
| `layout_get_command` | чтение раскладки (пусто = автопоиск) | — |
| `layout_set_command` | установка нужной раскладки, `{n}` — индекс | — |
| `layout_order` | порядок раскладок в системе | `en,ru` |
| `text_mapper` | `static` (встроенная таблица) или `xkb` | `static` |
| `output` | `auto` \| `uinput` \| `clipboard` \| `command` | `auto` |
| `type_command` | команда печати для `output = command` | `wtype {text}` |

Автопоиск раскладки проверяет по очереди: `g3kb-switch -p`, `xkb-switch -p`,
`qdbus … org.kde.KeyboardLayouts.getLayout`, `gsettings … mru-sources`.

## Формат словарей

Файлы в `~/.config/punto-rs/`: одна запись на строку `<ПРИЗНАКИ><TAB><текст>`.

| признаки | смысл |
|---|---|
| `PE` | настоящее слово языка: точное совпадение + «валидно здесь» |
| `B` | совпадение по префиксу, без признака «валидно здесь» |
| (пусто) | вхождение подстроки |
| `C` | учитывать регистр (`CapsLock`, `.net`, `A4Tech`) |
| `A`, `D` | дополнительные признаки (как в оригинале; `D` участвует в решении) |

Три файла работают вместе:

* `ru.dict` — русские слова, `PE`;
* `en.dict` — английские слова, `PE`;
* `wrong.dict` — те же русские слова, **набранные в EN-раскладке**, `B`.

Именно `wrong.dict` заставляет алгоритм сработать: «ghbdtn» совпадает без
признака «валидно здесь», а «привет» — с ним, поэтому логика выбирает
конвертацию. `data/build_dict.py` генерирует все три файла из hunspell.

## Пакет, служба, тесты

```bash
make build      # бинарник с evdev/uinput
make test       # 23 теста (ядро)
sh packaging/build-deb.sh                      # build/punto-rs_0.1.0_amd64.deb
cargo check --features "input-evdev xkb"       # режим xkbcommon
```

Пакет ставит `/usr/bin/punto-rs`, `punto-rs.service` (systemd --user),
udev-правило и словари в `/usr/share/punto-rs`:

```bash
sudo dpkg -i build/punto-rs_0.1.0_amd64.deb
sudo usermod -aG input "$USER"       # затем перелогиниться
systemctl --user enable --now punto-rs.service
```

Словари в пакете — только демонстрационные; полные соберите
`data/build_dict.py` (см. выше) или положите свои файлы в `~/.config/punto-rs/`.

## Wayland

Первое, что стоит сделать на машине: `punto doctor` — он скажет, что доступно.

Проверенные факты, на которых всё построено:

* **mutter не поддерживает `virtual-keyboard-unstable-v1`** (mutter#1974
  открыт), поэтому `wtype` на GNOME Wayland **не работает**;
* **`uinput` работает везде** — это уровень ядра, композитор видит
  виртуальную клавиатуру как обычную;
* **буфер обмена + Ctrl+V через uinput** работает независимо от раскладки;
* **смены раскладки через CLI у GNOME нет**: нужен `g3kb-switch` +
  расширение GNOME Shell; на KDE — D-Bus `org.kde.keyboard`; на
  sway/Hyprland — их команды.

### Три стратегии вставки

| стратегия | механика | когда выбирается |
|---|---|---|
| `uinput` | Backspace × N, затем печать кодами клавиш | есть рабочее переключение |
| `clipboard` | Backspace × N, `wl-copy`, Ctrl+V | переключения нет |
| `command` | Backspace × N, внешняя команда (`wtype {text}`) | wlroots (sway, Hyprland) |

`output = auto`: `command` → `uinput` → `clipboard`.

### GNOME Wayland: полное поведение

```bash
sudo apt install wl-clipboard libglib2.0-dev cmake
# собрать и установить g3kb-switch (CLI + расширение Shell), затем перезайти
gnome-extensions enable g3kb-switch@g3kb-switch.org
g3kb-switch -p        # должно печатать текущую группу
```
и в конфиге:

```ini
layout_get_command = g3kb-switch -p
layout_set_command = g3kb-switch -s {n}
```

Тогда слово перепечатывается **и** раскладка переключается, как в Windows.

### GNOME без расширения

Только `wl-clipboard`: демон выбирает `clipboard` — символы всегда верные,
но раскладка не переключается (каждое следующее слово нужно исправлять
заново).

### KDE и wlroots

```ini
; KDE Plasma (X11 и Wayland)
layout_get_command = qdbus org.kde.keyboard /Layouts org.kde.KeyboardLayouts.getLayout
layout_set_command = qdbus org.kde.keyboard /Layouts org.kde.KeyboardLayouts.setLayout {n}
; sway
layout_set_command = swaymsg input type:keyboard xkb_switch_layout next
; Hyprland
layout_set_command = hyprctl switchxkblayout all next
```

## Если не работает

| симптом | причина и что делать |
|---|---|
| `doctor`: «нет доступа к /dev/input» | `sudo usermod -aG input $USER`, перелогиниться |
| `doctor`: «нет /dev/uinput» | `sudo modprobe uinput` + правило из `packaging/` |
| раскладка не переключается | GNOME: `g3kb-switch` (проверить `g3kb-switch -p`); X11: `xkb-switch -p` |
| слово исправилось, но раскладка прежняя | это режим `clipboard`: нужен `g3kb-switch` |
| в терминале вставка не сработала | там `Ctrl+Shift+V`; задайте `output = uinput` |
| в поле пароля не работает | вставка запрещена — используйте `output = uinput` |
| печатаются не те символы | нестандартная раскладка: соберите с `--features xkb` и `text_mapper = xkb` |
| символы дублируются | демон запущен дважды (`systemctl --user status punto-rs` + `pgrep -a punto`) |
| ничего не происходит | клавиатура не в `/dev/input/event*` или нет прав — `punto doctor` |

## Ограничения

* нет доступа к тексту приложения (на Wayland его нет в принципе), поэтому
  длина слова берётся из собственного буфера: при активной автозамене или
  IME число Backspace может не совпасть;
* раскладка сверяется раз в 2 секунды — мгновенное ручное переключение
  может опередить демон на одно слово;
* списки приложений-исключений оригинала (`default-conf.json`) не перенесены;
* `triggers.dat` (редкие триггеры) не перенесён;
* поддерживаются две раскладки (`layout_order`), прочие — через `xkbcommon`;
* определение поля пароля — эвристика (`/ha`, цифры, спецсимволы), а не
  `IsPasswordField` из хук-модуля.

## Разработка

```bash
cargo test                                  # 23 теста ядра
cargo build --features input-evdev          # демон (Linux)
cargo check --features "input-evdev xkb"    # + xkbcommon
```

Структура и соответствие функциям оригинала — `docs/ARCHITECTURE.md`.
Разбор форматов Punto и алгоритма — `../punto-re/spec/`.

CI: `.github/workflows/ci.yml` — тесты и сборка обоих наборов фич на
`ubuntu-latest`. Сборка и прогон проверены также локально на Ubuntu 24.04
(Wayland-сессия): `cargo build --features input-evdev`,
`cargo check --features "input-evdev xkb"`, `cargo test`, `punto doctor`.

## Лицензия

MIT — см. `LICENSE`. Данные Punto Switcher (Яндекс) не используются:
словари собираются из свободных источников (`data/build_dict.py`).

Репозиторий: <https://github.com/rumata-ap/punto-rs>


