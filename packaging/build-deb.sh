#!/bin/sh
# Сборка .deb без debhelper: проще и не требует прав на debian/rules.
#   sh packaging/build-deb.sh
set -eu

cd "$(dirname "$0")/.."

VER=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
ARCH=amd64
ROOT=build/deb/punto-rs

echo "== сборка бинарника ($VER) =="
cargo build --release --features input-evdev

rm -rf build/deb
mkdir -p "$ROOT/DEBIAN" \
         "$ROOT/usr/bin" \
         "$ROOT/lib/systemd/user" \
         "$ROOT/etc/udev/rules.d" \
         "$ROOT/usr/share/doc/punto-rs" \
         "$ROOT/usr/share/punto-rs"

install -m 755 target/release/punto "$ROOT/usr/bin/punto-rs"
install -m 644 packaging/punto-rs.service "$ROOT/lib/systemd/user/"
install -m 644 packaging/99-punto-rs-uinput.rules "$ROOT/etc/udev/rules.d/"
install -m 644 data/ru.dict data/en.dict data/wrong.dict "$ROOT/usr/share/punto-rs/"
install -m 644 data/config.example.ini "$ROOT/usr/share/punto-rs/"
install -m 644 README.md docs/ARCHITECTURE.md "$ROOT/usr/share/doc/punto-rs/"

cat > "$ROOT/DEBIAN/control" <<EOF
Package: punto-rs
Version: $VER
Section: utils
Priority: optional
Architecture: $ARCH
Depends: libc6
Recommends: xkb-switch | g3kb-switch
Maintainer: Aleksandr Ponomarev <palexxvlad@yandex.ru>
Description: Punto-подобное переключение раскладки для Linux
 Автоматически переключает раскладку и перепечатывает слово,
 если оно набрано не в той раскладке. Алгоритм совместим с
 Punto Switcher 4.5 (две гипотезы + словарь с признаками).
EOF

cat > "$ROOT/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ -d /etc/udev/rules.d ]; then
    udevadm control --reload-rules 2>/dev/null || true
    udevadm trigger 2>/dev/null || true
fi
echo "punto-rs: добавьте себя в группу input и включите службу:"
echo "  sudo usermod -aG input \$USER"
echo "  systemctl --user enable --now punto-rs.service"
EOF
chmod 755 "$ROOT/DEBIAN/postinst"

cat > "$ROOT/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
systemctl --user stop punto-rs.service 2>/dev/null || true
EOF
chmod 755 "$ROOT/DEBIAN/prerm"

dpkg-deb --build --root-owner-group "$ROOT" "build/punto-rs_${VER}_${ARCH}.deb"
echo "готово: build/punto-rs_${VER}_${ARCH}.deb"
