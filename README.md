# Rust-FP-Eldrid

An improved, high-performance fork of `rust-fp` featuring native support for the **ChromeOS FpInfo V2** protocol, ultra-low latency verification (~150ms), and complete integration with modern Linux desktop environments (**GNOME & KDE Plasma**).

Developed and thoroughly tested on the Chromebook platform **"Eldrid"** (Intel 11th Gen Tiger Lake / HP Chromebook x360 14c and related motherboards equipped with ChromeOS FPMCU sensors).

---

## 🌟 Key Features & Improvements

1. **Native `FpInfo V2` Protocol Support (FPC / Tiger Lake Sensors)**:
   - Modern Chromebooks with ChromeOS FPMCU and FPC (Fingerprint Cards AB) sensors do not support the older V1 command protocol (`versions mask = 0xC`). The upstream `crosec` crate panicked with `fp doesn't support V1`.
   - Added full implementation and response decoding for V2 packets (196 bytes) supporting template sizes of 5156 bytes.

2. **Ultra-Low Latency Verification (~150ms Instant Match)**:
   - Eliminated redundant post-match 5KB template downloads over the slow SPI/EC bus, dropping authentication latency from ~1.5 seconds down to an instant ~150ms.
   - Prevents sensor RAM cache invalidation and avoids SELinux permission denials during login/unlock.

3. **Standard Security Architecture & Parallel Dual Auth**:
   - Follows modern OS security standards (matching Android, iOS, macOS Touch ID, and Windows Hello):
     - **Cold Boot (First Login):** Requires password entry to securely derive cryptographic keys and decrypt the user keyring (`gnome-keyring` / `kwallet`), preserving browser logins (Brave, Chrome, etc.) and system tokens.
     - **Daily Re-authentication:** Instant biometric unlocking on Lock Screen (`Super + L`), terminal `sudo`, and graphical Polkit elevation prompts.
   - Includes a lightweight D-Bus bridge (`cros-fprint-bridge.py`) that emulates `net.reactivated.Fprint`, enabling GNOME Shell to run parallel `gdm-fingerprint` and `gdm-password` workers.

4. **Automated Installer & Uninstaller**:
   - Zero-touch Bash scripts that install system binaries, configure D-Bus policies, set up systemd services, and manage the PAM stacks on Fedora and RPM-based distributions.

---

## 🖥️ Desktop Environment Compatibility

| Desktop Environment | Lock Screen Unlock | `sudo` in Terminal | Polkit GUI Prompts | First Boot (Login Screen) |
| :--- | :---: | :---: | :---: | :---: |
| **GNOME Shell (GDM)** | ✅ Instant Huella / Pass | ✅ Supported | ✅ Supported | 🔑 Password (Unlocks Keyring) |
| **KDE Plasma (SDDM)** | ✅ Supported (`kscreenlocker`) | ✅ Supported | ✅ Supported | 🔑 Password (Unlocks KWallet) |

### KDE Plasma Notes
* **D-Bus Integration:** KDE Plasma 5.24+ and Plasma 6 natively monitor `net.reactivated.Fprint` on the system bus for biometric features. The included `cros-fprint-bridge.service` satisfies KDE's hardware detection out of the box.
* **Lock Screen (`kscreenlocker`):** Add `auth sufficient librust_fp_pam_module.so` to `/etc/pam.d/kscreenlocker` to enable fingerprint unlock in KDE Plasma.
* **Polkit / Sudo:** Both `sudo` and KDE's authentication agent (`polkit-kde-authentication-agent-1`) rely on `/etc/pam.d/sudo` and `/etc/pam.d/polkit-1`, which are configured automatically by the installer.

---

## 👥 Acknowledgements & Credits

This project is an open-source fork built upon the excellent work of the community:

- **[ChocolateLoverRaj/rust-fp](https://github.com/ChocolateLoverRaj/rust-fp)** & **[crosec-rs](https://github.com/ChocolateLoverRaj/crosec-rs)**:
  Original author of the library, D-Bus service architecture, and `rust-fp` driver framework.
- **[yuexiaoyong-star/rust-fp](https://github.com/yuexiaoyong-star/rust-fp)**:
  Contributions towards GNOME Polkit agent helper integration and safe PAM handling.
- **[CarrsterX](https://github.com/CarrsterX)**:
  Reverse engineering and implementation of the `FpInfo V2` protocol, latency optimizations, GDM lock-screen PAM integration, D-Bus bridge emulation, and automated system deployment scripts.

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
- System binaries in `/usr/local/bin` (`rust-fp`, `rust-fp-dbus-interface`, `cros-fprint-bridge.py`).
- PAM module in `/lib64/security/librust_fp_pam_module.so`.
- D-Bus policy in `/usr/share/dbus-1/system.d/org.rust_fp.RustFp.conf`.
- Systemd daemon units `rust-fp-dbus-interface.service` and `cros-fprint-bridge.service`.
- PAM authentication stacks in `/etc/pam.d/sudo`, `/etc/pam.d/polkit-1`, and `/etc/pam.d/gdm-fingerprint`.

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
