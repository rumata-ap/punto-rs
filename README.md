# punto-rs — аналог Punto Switcher для Linux (Debian 13 / Ubuntu, X11 и Wayland)

Печатаете «ghbdtn» — получаете «привет»: слово набрано не в той раскладке,
и программа исправляет его сама. Алгоритм восстановлен реверс-инжинирингом
Punto Switcher 4.5.0.583 (см. `../punto-re/spec/algorithm.md`), словари —
из открытых источников.

## Состояние

| часть | статус |
|---|---|
| ядро: словарь, скорер, решение, буфер | готово, **40 тестов** |
| `punto convert` / `punto sim` / `punto doctor` | работают, проверено |
| Linux-слой: evdev → uinput | **собрано и проверено на Linux** (Ubuntu 24.04) |
| режим `xkbcommon` (`--features xkb`) | собирается на Linux |
| `punto tray` — иконка в трее (фича `tray`) | собирается, `ksni`/SNI без GTK; на GNOME нужен AppIndicator |
| смена раскладки | `xkb-switch` / `g3kb-switch` / KDE D-Bus / sway / Hyprland |
| Wayland | три стратегии вставки, полное поведение — с `g3kb-switch` |
| `triggers.dat` оригинала | не перенесён (влияет на единичные случаи) |

Что именно проверено на реальном Linux (Ubuntu 24.04, Wayland-сессия в WSL):

```
cargo build  --features input-evdev        → Finished (7.1 s)
cargo check  --features "input-evdev xkb"  → Finished (3.8 s)
cargo test                                 → 40 passed, 0 failed
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
| `gnome-shell-extension-appindicator` | показывать иконки трея на GNOME (`punto tray`) | для `punto tray` |
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

Проверить: `punto doctor` → `/dev/uinput: открывается на запись`. Без
udev-правила устройство остаётся `root:root 0600`, и демон умирает на старте
с `Permission denied`. Разовый `sudo chmod 660 /dev/uinput` работает только до
перезагрузки — после неё права сбрасываются, и нужен именно правило из
`packaging/` (см. `journalctl --user -u punto-rs`).

## Конфигурация

`~/.config/punto-rs/config.ini` (путь переопределяется `PUNTO_CONFIG`).
Полный пример — `data/config.example.ini`. Ключевые параметры:

| ключ | смысл | по умолчанию |
|---|---|---|
| `max_word_len` | максимум символов в слове | `32` |
| `detect_passwords` | не трогать похожее на пароль | `true` |
| `dont_convert_capitals` | не трогать слова с заглавной | `false` |
| `convert_hotkey` | VK-код ручной конвертации (`0` — выключено; `145` = ScrollLock, `19` = Pause; буквы/цифры нельзя) | `0` |
| `auto_convert` | конвертировать автоматически на границе слова (пробел, знак) | `false` |
| `convert_chord` | комбинация ручной конвертации: `ctrl+shift` (любая сторона) или `rctrl+rshift` (только правые); третья клавиша между ними отменяет комбинацию, а повторное нажатие возвращает слово обратно (циклически) | `ctrl+shift` |
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
make test       # юнит- и интеграционные тесты (без фич — переносимо)
cargo test --features "input-evdev,tray"       # весь набор, включая io и трей
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

## Автозапуск при входе в систему

Демон — юнит systemd **пользователя**, привязанный к графической сессии:
раньше сессии ему нечего делать (нет ни раскладки, ни `/dev/uinput`).
Юнит включён в `graphical-session.target`, поэтому поднимается при каждом входе.

**Одной командой, без sudo** — бинарник, юниты, конфиг и словари в домашний
каталог:

```bash
make install-user        # ~/.local/bin/punto-rs + ~/.config/systemd/user/*.service
                         # + ~/.config/punto-rs/{config.ini,*.dict}, enable --now
make uninstall-user      # обратно (конфиг и словари остаются)
```

Поведение свитчера постоянно по двум причинам:

* **настройки по умолчанию уже рабочие** — `convert_chord = ctrl+shift`,
  `convert_hotkey = 0`, `auto_convert = false`: конфиг можно вообще не заводить
  (`Config::load` на отсутствующем файле отдаёт те же значения);
* юнит `enabled`, поэтому при входе в систему демон стартует сам, а
  `Restart=always` поднимает его после падения.

Вручную то же самое:

```bash
# из пакета / make install (юнит в /usr/local/lib/systemd/user)
systemctl --user enable --now punto-rs.service

# свой бинарник в ~/.local/bin
install -Dm644 packaging/punto-rs.service ~/.config/systemd/user/punto-rs.service
systemctl --user daemon-reload
systemctl --user enable --now punto-rs.service

# проверка
systemctl --user is-enabled punto-rs     # enabled
systemctl --user is-active punto-rs      # active
journalctl --user -u punto-rs -f         # «читаю N клавиатур(ы)», «вывод текста — uinput»
```

Права на `/dev/uinput` после перезагрузки даёт udev-правило (иначе `chmod`
живёт до перезагрузки):

```bash
sudo cp packaging/99-punto-rs-uinput.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

Почему это надёжно при входе в систему:

| настройка | зачем |
|---|---|
| `WantedBy=`/`After=`/`PartOf=graphical-session.target` | старт и остановка вместе с сессией |
| `ExecStartPre` ждёт `g3kb-switch -p` (до 60 с) | на входе расширение GNOME Shell ещё не готово |
| `Restart=always` + `RestartSec=3` | демон поднимается после падения или `kill -9` |
| `StartLimitIntervalSec=0` | systemd не «сдаётся» после 5 падений за 10 с |
| `exit(1)` при ошибке старта | с кодом 0 systemd считает запуск удачным и не перезапускает |

Строка `автозапуск` в `punto doctor` показывает текущее состояние юнита
(`enabled` / `disabled` / `not-found`).

Проверить самовосстановление: `kill -9 $(pgrep -f 'punto-rs run')` — через
3 секунды сервис снова `active`, `NRestarts` в `systemctl --user show` растёт.
Если сессия уже была без демона: `systemctl --user reset-failed punto-rs &&
systemctl --user start punto-rs`.

## Трей: иконка как в Windows

Опциональная фича `tray` — значок в системном трее с меню (реализация
KDE/freedesktop StatusNotifierItem через крейт `ksni`, **без GTK** и
`libappindicator`):

```bash
cargo build --release --features "input-evdev,tray"   # или: make build-tray
./target/release/punto tray                            # иконка в трее
# автозапуск вместе с сессией
install -Dm644 packaging/punto-tray.service ~/.config/systemd/user/
systemctl --user daemon-reload && systemctl --user enable --now punto-tray
```

Меню (как у Punto Switcher в Windows):

| пункт | что делает |
|---|---|
| `punto-rs: работает · раскладка ru` | состояние: сервис, текущая раскладка |
| `вывод: uinput · автоправка: выкл` | стратегия вставки, режим правки, ошибки действий |
| ☑ `Демон запущен` | `systemctl --user start/stop punto-rs` |
| ☑ `Автоправка по пробелу` | меняет `auto_convert` в `config.ini` (комментарии сохраняются) и перезапускает демон |
| `Преобразовать слово` (комбинация из `convert_chord`) | шлёт комбинацию через uinput — демон исправляет последнее слово |
| `Ещё → Проверить среду` | запускает `punto doctor` и показывает результат уведомлением |
| `Ещё → Настройки` | открывает `config.ini` |
| `Ещё → Журнал демона` | `journalctl --user -u punto-rs` в терминале |
| `Перезапустить демон` / `Выход` | — |

Иконка: 22×22, две половины (синяя EN / красная RU), серая — когда демон
остановлен.

**На GNOME иконки SNI не видны без расширения** — это ограничение оболочки:

```bash
sudo apt install gnome-shell-extension-appindicator
gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com   # затем перелогиниться
punto doctor        # строка «трей»: StatusNotifierWatcher есть
```

KDE Plasma показывает иконку штатно, sway/Hyprland — через waybar (`tray`) или
`snixembed`. Если `StatusNotifierWatcher` нет, `punto tray` завершится с
подсказкой, а `doctor` покажет `warn`.

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
| `doctor`: «нет прав на запись» у `/dev/uinput` | правило не установлено: `sudo cp packaging/99-punto-rs-uinput.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger`, перелогиниться (разово — `sudo chgrp input /dev/uinput && sudo chmod 660 /dev/uinput`) |
| `doctor`: «нет /dev/uinput» | `sudo modprobe uinput` + правило из `packaging/` |
| работало, но после перезагрузки перестало | `chmod`/`chgrp` на `/dev/uinput` живут до перезагрузки; нужно именно udev-правило из `packaging/` |
| раскладка не переключается | GNOME: `g3kb-switch` (проверить `g3kb-switch -p`); X11: `xkb-switch -p` |
| в журнале `Failed to activate layout N` | на GNOME 48 `g3kb-switch -s N` не работает (у `-n` работает): демон сам откатывается на `switch_command`; строку «установка раскладки» в `punto doctor` смотреть там же |
| текст перемешивается: буквы прыгают в другую раскладку прямо при наборе | в `convert_hotkey` попала буква: старый дефолт `70` — это VK_F, а не ScrollLock. Поставьте `convert_hotkey = 0` (в журнале будет строка «ВНИМАНИЕ convert_hotkey = … это буква») |
| слово не исправилось, в журнале «раскладку на … переключить не удалось» | переключение не сработало ни через `layout_set_command`, ни через `switch_command` — проверьте обе команды вручную |
| иконки в трее нет, `punto tray` ругается на `StatusNotifierWatcher` | на GNOME нет расширения AppIndicator: `sudo apt install gnome-shell-extension-appindicator` (или скачать вручную), `gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com`, перелогиниться; `punto doctor` покажет строку «трей» |
| после входа в систему демона нет | `systemctl --user reset-failed punto-rs && systemctl --user start punto-rs`, затем `journalctl --user -u punto-rs` (автозапуск — раздел «Автозапуск при входе в систему») |
| слово исправилось, но раскладка прежняя | это режим `clipboard`: нужен `g3kb-switch` |
| в терминале вставка не сработала | там `Ctrl+Shift+V`; задайте `output = uinput` |
| в поле пароля не работает | вставка запрещена — используйте `output = uinput` |
| печатаются не те символы | нестандартная раскладка: соберите с `--features xkb` и `text_mapper = xkb` |
| символы дублируются | демон запущен дважды (`systemctl --user status punto-rs` + `pgrep -a punto`) |
| ничего не происходит | клавиатура не в `/dev/input/event*` или нет прав — `punto doctor` |
| нажимаю Ctrl+Shift, слово не исправляется | смотрите `journalctl --user -u punto-rs -n 20`: после комбинации демон пишет «ручная конвертация: …» с решением. «пустое слово» — возвращать нечего: буфер пуст и слово ещё не перепечатывалось (набрали пробел/Enter/знак или другое слово), «оставить» — словарь не увидел смысла, «раскладку на … переключить не удалось» — виновата команда переключения. Если строки нет вовсе — демон не услышал клавиатуру (см. ниже) |
| повторное Ctrl+Shift со словом не срабатывает | слово возвращается обратно, только пока не набрано что-то новое: любой символ, пробел или Enter сбрасывают цикл («пустое слово» в журнале) |
| комбинация не срабатывает от левых Ctrl+Shift | в конфиге указаны правые: `convert_chord = rctrl+rshift`. Замените на `ctrl+shift` — тогда подойдёт любая сторона |
| после Ctrl+Shift+… (вставка, копирование) слово не тронуто | так и задумано: третья клавиша между модификаторами отменяет комбинацию |
| демон «active», но клавиатура не слышна | поток чтения устройства умер (USB-переподключение, сон): теперь он переоткрывает устройство и пишет «ошибка чтения …, переоткрываю», а подключённые позже клавиатуры подхватываются заново («новая клавиатура — …»). `systemctl --user restart punto-rs` |

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
cargo test                                  # 40 тестов
cargo build --features input-evdev          # демон (Linux)
cargo check --features "input-evdev xkb"    # + xkbcommon
cargo test  --features "input-evdev,tray"   # 48 тестов, включая трей
cargo build --release --features "input-evdev,tray"   # демон + `punto tray`
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


