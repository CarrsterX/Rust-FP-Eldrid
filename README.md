# Rust-FP-Eldrid

An improved version of `rust-fp` featuring native support for the **FpInfo V2** protocol and complete integration with **GNOME (sudo, Polkit, and GDM lock screen)**, developed and tested on the Chromebook **"Eldrid"** (Intel 11th Gen Tiger Lake / HP Chromebook x360 14c and similar motherboards equipped with ChromeOS FPMCU).

---

## 🌟 Key Features & Improvements

1. **Native `FpInfo V2` Protocol Support (FPC / Tiger Lake Sensors)**:
   - Modern Chromebooks with ChromeOS FPMCU and FPC (Fingerprint Cards AB) sensors do not support the older V1 command protocol (`versions mask = 0xC`). The upstream `crosec` crate panicked with `fp doesn't support V1`.
   - Added full implementation and response decoding for V2 packets (196 bytes) supporting template sizes of 5156 bytes.
2. **Native GDM Dual-Authentication (Parallel Password OR Fingerprint)**:
   - Built-in lightweight D-Bus bridge (`cros-fprint-bridge.py`) emulating standard `net.reactivated.Fprint` properties.
   - Enables GNOME Display Manager (GDM) and lock screen to run `gdm-fingerprint` and `gdm-password` in parallel:
     - Type password and press Enter at any time -> logs in instantly and decrypts GNOME Keyring (preserving browser logins and saved passwords).
     - Or touch the fingerprint sensor -> logs in instantly without touching the keyboard.
   - Out-of-the-box support for GNOME graphical authentication dialogs (**Polkit / `pkexec`**).
   - Fully compatible with **`sudo`** authentication in terminal sessions.
3. **Automated Installer & Uninstaller**:
   - Bash scripts that handle installing binaries, setting up D-Bus permissions, configuring systemd services, and managing the PAM stack on Fedora and RPM-based distributions.

---

## 👥 Acknowledgements & Credits

This project is an open-source fork built upon the excellent work of the community:

- **[ChocolateLoverRaj/rust-fp](https://github.com/ChocolateLoverRaj/rust-fp)** & **[crosec-rs](https://github.com/ChocolateLoverRaj/crosec-rs)**:
  Original author of the library, D-Bus service architecture, and `rust-fp` driver framework.
- **[yuexiaoyong-star/rust-fp](https://github.com/yuexiaoyong-star/rust-fp)**:
  Contributions towards GNOME Polkit agent helper integration and safe PAM handling.
- **[CarrsterX](https://github.com/CarrsterX)**:
  Reverse engineering and implementation of the `FpInfo V2` protocol, GDM lock-screen PAM integration, and automated system deployment scripts.

---

## 📋 Prerequisites

On Fedora / RHEL / derivatives:
```bash
sudo dnf install -y git cargo gcc pam-devel
```

---

## 🚀 Build & Installation

### 1. Clone the repository
```bash
git clone https://github.com/CarrsterX/Rust-FP-Eldrid.git
cd Rust-FP-Eldrid
```

### 2. Build binaries
```bash
cargo build --release
```

### 3. Install to system
Run the automated installation script with root privileges:
```bash
sudo ./install_fingerprint.sh
```

The installer configures:
- System binaries in `/usr/local/bin` (`rust-fp`, `rust-fp-dbus-interface`).
- PAM module in `/lib64/security/librust_fp_pam_module.so`.
- D-Bus policy in `/usr/share/dbus-1/system.d/org.rust_fp.RustFp.conf`.
- Systemd daemon unit `rust-fp-dbus-interface.service`.
- D-Bus emulation bridge service `cros-fprint-bridge.service`.
- PAM authentication stacks in `/etc/pam.d/sudo`, `/etc/pam.d/polkit-1`, and parallel `/etc/pam.d/gdm-fingerprint`.

---

## 🖐️ Usage & Management

Manage your enrolled fingerprints as a standard user (**without `sudo`**):

### Enroll a new finger
```bash
rust-fp add index
```
Follow the on-screen instructions, placing your finger on the sensor until completion reaches 100%.

### List saved fingerprints
```bash
rust-fp list
```

### Test authentication in terminal
```bash
sudo -k && sudo whoami
```
Touch the fingerprint sensor when prompted to authorize.

### Screen unlock
Lock your screen (`Super + L`) and touch the enrolled sensor to instantly unlock.

### Remove fingerprints
- To delete a specific fingerprint:
  ```bash
  rust-fp remove index
  ```
- To clear all stored templates:
  ```bash
  rust-fp clear
  ```

---

## 🧹 Uninstallation

To revert all system configurations and restore original PAM files:
```bash
sudo ./uninstall_fingerprint.sh
```
