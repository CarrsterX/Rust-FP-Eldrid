#!/usr/bin/env bash
# install_fingerprint.sh
# ChromeOS Fingerprint Sensor (CrOS FPMCU / cros_fp) Support Installer for Fedora / GNOME

set -e

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ "$EUID" -ne 0 ]; then
    echo "[!] Superuser privileges required."
    echo "[*] Elevating: sudo bash $0 $@"
    exec sudo bash "$0" "$@"
fi

echo "=================================================================="
echo " FINGERPRINT SENSOR INSTALLER (CROS-FP / RUST-FP FOR GNOME) "
echo "=================================================================="

# 1. Verify compiled binaries
BIN_DIR="$DIR/target/release"
if [ ! -f "$BIN_DIR/rust-fp" ] || [ ! -f "$BIN_DIR/rust-fp-dbus-interface" ] || [ ! -f "$BIN_DIR/librust_fp_pam_module.so" ]; then
    echo "[!] Error: Compiled binaries not found in $BIN_DIR"
    echo "    Please build project first using: cargo build --release"
    exit 1
fi

# 2. Stop running service and copy binaries
echo "[*] Stopping active daemon if running..."
systemctl stop rust-fp-dbus-interface.service 2>/dev/null || true

echo "[*] Installing system binaries..."
cp --remove-destination "$BIN_DIR/rust-fp" /usr/local/bin/rust-fp
chmod 755 /usr/local/bin/rust-fp

cp --remove-destination "$BIN_DIR/rust-fp-dbus-interface" /usr/local/bin/rust-fp-dbus-interface
chmod 755 /usr/local/bin/rust-fp-dbus-interface

cp --remove-destination "$BIN_DIR/librust_fp_pam_module.so" /lib64/security/librust_fp_pam_module.so
chmod 755 /lib64/security/librust_fp_pam_module.so

# 3. Install D-Bus system policy
echo "[*] Configuring D-Bus policy..."
cp "$DIR/dbus-interface/org.rust_fp.RustFp.conf" /usr/share/dbus-1/system.d/org.rust_fp.RustFp.conf
chmod 644 /usr/share/dbus-1/system.d/org.rust_fp.RustFp.conf

# 4. Deploy and start systemd service
echo "[*] Installing systemd unit rust-fp-dbus-interface..."
cat << 'UNIT' > /etc/systemd/system/rust-fp-dbus-interface.service
[Unit]
Description=D-Bus Service for ChromeOS Fingerprint Sensor (FPMCU)
After=dbus.service

[Service]
ExecStart=/usr/local/bin/rust-fp-dbus-interface
Type=exec
Restart=on-failure
RestartSec=2s

[Install]
WantedBy=multi-user.target
UNIT

chmod 644 /etc/systemd/system/rust-fp-dbus-interface.service

# 5. Configure polkit-agent-helper sandbox override for template access in $HOME
echo "[*] Configuring Polkit service sandbox override (ProtectHome=read-only)..."
mkdir -p /etc/systemd/system/polkit-agent-helper@.service.d
cat << 'DROPIN' > /etc/systemd/system/polkit-agent-helper@.service.d/override.conf
[Service]
ProtectHome=read-only
DROPIN

systemctl daemon-reload
systemctl enable --now rust-fp-dbus-interface.service
systemctl restart rust-fp-dbus-interface.service

# 6. Configure PAM for sudo
echo "[*] Configuring PAM for sudo (/etc/pam.d/sudo)..."
if [ ! -f /etc/pam.d/sudo.orig ]; then
    cp /etc/pam.d/sudo /etc/pam.d/sudo.orig
fi

if ! grep -q "librust_fp_pam_module.so" /etc/pam.d/sudo; then
    sed -i '/^auth /i auth       sufficient   librust_fp_pam_module.so' /etc/pam.d/sudo
    echo "[+] Added fingerprint auth rule to /etc/pam.d/sudo"
else
    echo "[*] Fingerprint auth rule already present in /etc/pam.d/sudo"
fi

# 7. Configure PAM for polkit-1 (GNOME graphical privilege elevation prompts)
echo "[*] Configuring PAM for Polkit (/etc/pam.d/polkit-1)..."
if [ ! -f /etc/pam.d/polkit-1 ]; then
    cat << 'POLKIT' > /etc/pam.d/polkit-1
#%PAM-1.0
auth       sufficient   librust_fp_pam_module.so
auth       include      system-auth
account    include      system-auth
password   include      system-auth
session    include      system-auth
POLKIT
    chmod 644 /etc/pam.d/polkit-1
    echo "[+] Created /etc/pam.d/polkit-1 with fingerprint auth support"
else
    if ! grep -q "librust_fp_pam_module.so" /etc/pam.d/polkit-1; then
        sed -i '/^auth /i auth       sufficient   librust_fp_pam_module.so' /etc/pam.d/polkit-1
        echo "[+] Added fingerprint auth rule to existing /etc/pam.d/polkit-1"
    fi
fi

echo ""

# 8. Configure PAM for GDM / GNOME Lock Screen
echo "[*] Configuring PAM for GDM / Lock Screen (/etc/pam.d/gdm-password)..."
if [ ! -f /etc/pam.d/gdm-password ]; then
    cp /usr/lib/pam.d/gdm-password /etc/pam.d/gdm-password
    sed -i "/gdm-password-auth-substack/i auth       sufficient   librust_fp_pam_module.so" /etc/pam.d/gdm-password
    chmod 644 /etc/pam.d/gdm-password
    echo "[+] Configured /etc/pam.d/gdm-password with fingerprint auth support"
else
    if ! grep -q "librust_fp_pam_module.so" /etc/pam.d/gdm-password; then
        sed -i "/gdm-password-auth-substack/i auth       sufficient   librust_fp_pam_module.so" /etc/pam.d/gdm-password
        echo "[+] Added fingerprint auth rule to existing /etc/pam.d/gdm-password"
    fi
fi

echo "=================================================================="
echo " INSTALLATION COMPLETED SUCCESSFULLY! "
echo "=================================================================="
echo "Service Status:"
systemctl status rust-fp-dbus-interface.service --no-pager || true
echo ""
echo "NEXT STEPS (Run as normal user, WITHOUT sudo):"
echo "  1. Enroll fingerprint template:"
echo "       rust-fp add index"
echo "     (Follow on-screen instructions while placing finger on sensor)"
echo ""
echo "  2. Test authentication in terminal:"
echo "       sudo -k && sudo whoami"
echo "     (Prompt will request sensor touch or password fallback)"
echo "=================================================================="
