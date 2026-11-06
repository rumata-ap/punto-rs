PREFIX ?= /usr/local
BINDIR  = $(DESTDIR)$(PREFIX)/bin
UNITDIR = $(DESTDIR)$(PREFIX)/lib/systemd/user
DOCDIR  = $(DESTDIR)$(PREFIX)/share/doc/punto-rs
DATADIR = $(DESTDIR)$(PREFIX)/share/punto-rs

.PHONY: all build test deb install uninstall clean

all: build

build:
	cargo build --release --features input-evdev

test:
	cargo test

# .deb без debhelper
deb:
	sh packaging/build-deb.sh

install: build
	install -d $(BINDIR) $(UNITDIR) $(DOCDIR) $(DATADIR)
	install -m 755 target/release/punto $(BINDIR)/punto-rs
	install -m 644 packaging/punto-rs.service $(UNITDIR)/
	install -m 644 data/ru.dict data/en.dict data/wrong.dict $(DATADIR)/
	install -m 644 data/config.example.ini $(DATADIR)/
	install -m 644 README.md docs/ARCHITECTURE.md $(DOCDIR)/
	@echo "дальше: sudo cp packaging/99-punto-rs-uinput.rules /etc/udev/rules.d/"
	@echo "        systemctl --user enable --now punto-rs.service"

uninstall:
	rm -f $(BINDIR)/punto-rs
	rm -rf $(DATADIR) $(DOCDIR)
	rm -f $(UNITDIR)/punto-rs.service

clean:
	cargo clean
	rm -rf build
