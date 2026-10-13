#!/usr/bin/env bash
# uninstall_fingerprint.sh
# Uninstaller and configuration restoration script for ChromeOS fingerprint support

set -e

if [ "$EUID" -ne 0 ]; then
    echo "[!] Superuser privileges required."
    exec sudo bash "$0" "$@"
fi

echo "[*] Disabling and removing systemd service..."
systemctl disable --now rust-fp-dbus-interface.service 2>/dev/null || true
rm -f /etc/systemd/system/rust-fp-dbus-interface.service
rm -rf /etc/systemd/system/polkit-agent-helper@.service.d
systemctl daemon-reload

echo "[*] Removing installed binaries..."
rm -f /usr/local/bin/rust-fp
rm -f /usr/local/bin/rust-fp-dbus-interface
rm -f /lib64/security/librust_fp_pam_module.so
rm -f /usr/share/dbus-1/system.d/org.rust_fp.RustFp.conf

echo "[*] Restoring PAM authentication configurations..."
if [ -f /etc/pam.d/sudo.orig ]; then
    mv /etc/pam.d/sudo.orig /etc/pam.d/sudo
else
    sed -i '/librust_fp_pam_module.so/d' /etc/pam.d/sudo
fi

rm -f /etc/pam.d/polkit-1
rm -f /etc/pam.d/gdm-password

echo "[+] Fingerprint service uninstalled and configurations restored."
