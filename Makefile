PREFIX ?= /usr/local
BINDIR  = $(DESTDIR)$(PREFIX)/bin
UNITDIR = $(DESTDIR)$(PREFIX)/lib/systemd/user
DOCDIR  = $(DESTDIR)$(PREFIX)/share/doc/punto-rs
DATADIR = $(DESTDIR)$(PREFIX)/share/punto-rs

.PHONY: all build build-tray test deb install install-user uninstall uninstall-user clean

all: build

build:
	cargo build --release --features input-evdev

# демон + иконка в трее (StatusNotifierItem, нужен AppIndicator в GNOME)
build-tray:
	cargo build --release --features "input-evdev,tray"

test:
	cargo test

# .deb без debhelper
deb:
	sh packaging/build-deb.sh

install: build
	install -d $(BINDIR) $(UNITDIR) $(DOCDIR) $(DATADIR)
	install -m 755 target/release/punto $(BINDIR)/punto-rs
	install -m 644 packaging/punto-rs.service $(UNITDIR)/
	install -m 644 packaging/punto-tray.service $(UNITDIR)/
	install -m 644 data/ru.dict data/en.dict data/wrong.dict $(DATADIR)/
	install -m 644 data/config.example.ini $(DATADIR)/
	install -m 644 README.md docs/ARCHITECTURE.md $(DOCDIR)/
	@echo "дальше: sudo cp packaging/99-punto-rs-uinput.rules /etc/udev/rules.d/"
	@echo "        systemctl --user daemon-reload && systemctl --user enable --now punto-rs.service"

uninstall:
	rm -f $(BINDIR)/punto-rs
	rm -rf $(DATADIR) $(DOCDIR)
	rm -f $(UNITDIR)/punto-rs.service
	rm -f $(UNITDIR)/punto-tray.service

# Установка без sudo — всё в домашнем каталоге пользователя.
# Именно этот режим делает поведение постоянным: бинарник в ~/.local/bin,
# юниты systemd --user, конфиг и словари в ~/.config/punto-rs — при входе
# в систему демон поднимается сам, настраивать ничего не нужно.
USER_BIN = $(HOME)/.local/bin
USER_UNITS = $(HOME)/.config/systemd/user
USER_CONF = $(HOME)/.config/punto-rs

install-user:
	cargo build --release --features "input-evdev,tray"
	install -d $(USER_BIN) $(USER_UNITS) $(USER_CONF)
	install -m 755 target/release/punto $(USER_BIN)/punto-rs
	# домашняя установка берёт юниты с %h/.local/bin и БЕЗ SupplementaryGroups:
	# user-менеджер не может менять группы (systemd падает с 216/GROUP)
	install -m 644 packaging/punto-rs.user.service $(USER_UNITS)/punto-rs.service
	install -m 644 packaging/punto-tray.user.service $(USER_UNITS)/punto-tray.service
	@test -f $(USER_CONF)/config.ini || \
		(echo "конфиг: $(USER_CONF)/config.ini из data/config.example.ini" && \
		 install -m 644 data/config.example.ini $(USER_CONF)/config.ini)
	@for d in ru en wrong; do \
		test -f $(USER_CONF)/$$d.dict || install -m 644 data/$$d.dict $(USER_CONF)/$$d.dict; \
	done
	systemctl --user daemon-reload
	systemctl --user enable --now punto-rs.service
	@echo "готово: демон включён в автозапуск, ручная конвертация — из config.ini"
	@echo "проверка: systemctl --user is-enabled punto-rs && journalctl --user -u punto-rs -n 5"

uninstall-user:
	-systemctl --user disable --now punto-rs.service punto-tray.service
	rm -f $(USER_BIN)/punto-rs
	rm -f $(USER_UNITS)/punto-rs.service $(USER_UNITS)/punto-tray.service
	-systemctl --user daemon-reload
	@echo "конфиг и словари в $(USER_CONF) оставлены (удалите вручную, если не нужны)"

clean:
	cargo clean
	rm -rf build
