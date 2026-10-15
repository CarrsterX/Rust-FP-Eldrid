#!/usr/bin/env python3
"""
ChromeOS FPMCU to Fprint D-Bus Bridge for GNOME Shell, GDM, and KDE Plasma.
Implements net.reactivated.Fprint.Manager and net.reactivated.Fprint.Device
so that:
1. GDM / GNOME Shell enables parallel gdm-fingerprint PAM authentication.
2. GNOME Settings (gnome-control-center) and KDE System Settings can list,
   enroll, and delete fingerprints via native UI dialogs.
"""

import os
import sys
import pwd
import struct
import subprocess
import threading
import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

DBusGMainLoop(set_as_default=True)

STANDARD_FINGERS = [
    "right-thumb",
    "right-index-finger",
    "right-middle-finger",
    "right-ring-finger",
    "right-little-finger",
    "left-thumb",
    "left-index-finger",
    "left-middle-finger",
    "left-ring-finger",
    "left-little-finger",
]

FINGER_ALIASES = {
    "indice": "right-index-finger",
    "index": "right-index-finger",
    "right-index": "right-index-finger",
    "pulgar": "right-thumb",
    "thumb": "right-thumb",
    "right-thumb": "right-thumb",
    "left-index": "left-index-finger",
    "left-thumb": "left-thumb",
}

def parse_msgpack_keys(filepath):
    """Parse keys from rmp_serde / msgpack encoded template file without external deps."""
    try:
        with open(filepath, 'rb') as f:
            data = f.read()
    except Exception:
        return []

    if not data:
        return []

    try:
        pos = 0
        first = data[pos]
        pos += 1
        if 0x80 <= first <= 0x8F:
            count = first - 0x80
        elif first == 0xDE:
            count = struct.unpack_from('>H', data, pos)[0]
            pos += 2
        elif first == 0xDF:
            count = struct.unpack_from('>I', data, pos)[0]
            pos += 4
        else:
            return []

        keys = []
        for _ in range(count):
            if pos >= len(data):
                break
            k_byte = data[pos]
            pos += 1
            if 0xA0 <= k_byte <= 0xBF:
                k_len = k_byte - 0xA0
            elif k_byte == 0xD9:
                k_len = data[pos]; pos += 1
            elif k_byte == 0xDA:
                k_len = struct.unpack_from('>H', data, pos)[0]; pos += 2
            elif k_byte == 0xDB:
                k_len = struct.unpack_from('>I', data, pos)[0]; pos += 4
            else:
                break
            key_str = data[pos:pos+k_len].decode('utf-8', errors='ignore')
            pos += k_len
            keys.append(key_str)
            # Skip binary template value
            if pos >= len(data):
                break
            v_byte = data[pos]; pos += 1
            if v_byte == 0xC4:
                v_len = data[pos]; pos += 1
            elif v_byte == 0xC5 or v_byte == 0xDC:
                v_len = struct.unpack_from('>H', data, pos)[0]; pos += 2
            elif v_byte == 0xC6 or v_byte == 0xDD:
                v_len = struct.unpack_from('>I', data, pos)[0]; pos += 4
            else:
                v_len = 0
            pos += v_len
        return keys
    except Exception as e:
        sys.stderr.write(f"cros-fprint-bridge: error parsing {filepath}: {e}\n")
        return []

def get_user_template_file(username):
    try:
        pw = pwd.getpwnam(username)
        return os.path.join(pw.pw_dir, ".var", "cros-fp-templates")
    except KeyError:
        return None


class FprintDevice(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, '/net/reactivated/Fprint/Device/0')
        self.bus = bus
        self.claimed_user = None
        self._enroll_proc = None
        self._enroll_thread = None
        self._enroll_stop_requested = False

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='s', out_signature='as')
    def ListEnrolledFingers(self, username):
        tpl_path = get_user_template_file(username)
        if not tpl_path or not os.path.exists(tpl_path):
            return dbus.Array([], signature='s')

        raw_keys = parse_msgpack_keys(tpl_path)
        if not raw_keys:
            return dbus.Array([], signature='s')

        fingers = []
        for k in raw_keys:
            mapped = FINGER_ALIASES.get(k.lower(), k)
            if mapped in STANDARD_FINGERS:
                if mapped not in fingers:
                    fingers.append(mapped)
            else:
                for std in STANDARD_FINGERS:
                    if std not in fingers:
                        fingers.append(std)
                        break

        return dbus.Array([dbus.String(f) for f in fingers], signature='s')

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='s', out_signature='')
    def Claim(self, username):
        self.claimed_user = username

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='', out_signature='')
    def Release(self):
        self.EnrollStop()
        self.claimed_user = None

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='s', out_signature='')
    def VerifyStart(self, finger_name):
        pass

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='', out_signature='')
    def VerifyStop(self):
        pass

    @dbus.service.signal('net.reactivated.Fprint.Device', signature='sb')
    def EnrollStatus(self, result, done):
        pass

    @dbus.service.signal('net.reactivated.Fprint.Device', signature='sb')
    def VerifyStatus(self, result, done):
        pass

    @dbus.service.signal('net.reactivated.Fprint.Device', signature='s')
    def VerifyFingerSelected(self, finger_name):
        pass

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='s', out_signature='')
    def EnrollStart(self, finger_name):
        user = self.claimed_user or 'carrster'
        self.EnrollStop()
        self._enroll_stop_requested = False
        self._enroll_thread = threading.Thread(
            target=self._run_enroll_worker,
            args=(user, finger_name),
            daemon=True
        )
        self._enroll_thread.start()

    def _run_enroll_worker(self, username, finger_name):
        try:
            pw = pwd.getpwnam(username)
        except KeyError:
            GLib.idle_add(self.EnrollStatus, "enroll-unknown-error", True)
            return

        env = os.environ.copy()
        env["HOME"] = pw.pw_dir
        env["USER"] = username

        # If finger already exists in template file, remove it first
        subprocess.run(
            ["/usr/local/bin/rust-fp", "remove", finger_name],
            env=env,
            user=pw.pw_uid,
            group=pw.pw_gid,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL
        )
        if finger_name == "right-index-finger":
            subprocess.run(
                ["/usr/local/bin/rust-fp", "remove", "indice"],
                env=env,
                user=pw.pw_uid,
                group=pw.pw_gid,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL
            )

        cmd = ["/usr/local/bin/rust-fp", "add", finger_name]
        try:
            self._enroll_proc = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
                env=env,
                user=pw.pw_uid,
                group=pw.pw_gid
            )
        except Exception as e:
            sys.stderr.write(f"cros-fprint-bridge: failed to launch enroll: {e}\n")
            GLib.idle_add(self.EnrollStatus, "enroll-unknown-error", True)
            return

        for line in self._enroll_proc.stdout:
            if self._enroll_stop_requested:
                break
            line_str = line.strip()
            if not line_str:
                continue

            if "Enroll progress:" in line_str:
                GLib.idle_add(self.EnrollStatus, "enroll-stage-passed", False)
            elif "Try again" in line_str:
                GLib.idle_add(self.EnrollStatus, "enroll-retry-scan", False)
            elif "Enroll complete" in line_str:
                GLib.idle_add(self.EnrollStatus, "enroll-completed", True)
                break

        self._enroll_proc.wait()
        self._enroll_proc = None

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='', out_signature='')
    def EnrollStop(self):
        self._enroll_stop_requested = True
        if self._enroll_proc:
            try:
                self._enroll_proc.terminate()
                self._enroll_proc.wait(timeout=0.5)
            except Exception:
                try:
                    self._enroll_proc.kill()
                except Exception:
                    pass
            self._enroll_proc = None

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='s', out_signature='')
    def DeleteEnrolledFinger(self, finger_name):
        user = self.claimed_user or 'carrster'
        try:
            pw = pwd.getpwnam(user)
            env = os.environ.copy()
            env["HOME"] = pw.pw_dir
            env["USER"] = user
            subprocess.run(
                ["/usr/local/bin/rust-fp", "remove", finger_name],
                env=env,
                user=pw.pw_uid,
                group=pw.pw_gid,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL
            )
            if finger_name == "right-index-finger":
                subprocess.run(
                    ["/usr/local/bin/rust-fp", "remove", "indice"],
                    env=env,
                    user=pw.pw_uid,
                    group=pw.pw_gid,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL
                )
        except Exception as e:
            sys.stderr.write(f"cros-fprint-bridge: DeleteEnrolledFinger error: {e}\n")

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='s', out_signature='')
    def DeleteEnrolledFingers(self, username):
        try:
            pw = pwd.getpwnam(username)
            env = os.environ.copy()
            env["HOME"] = pw.pw_dir
            env["USER"] = username
            subprocess.run(
                ["/usr/local/bin/rust-fp", "clear"],
                env=env,
                user=pw.pw_uid,
                group=pw.pw_gid,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL
            )
        except Exception as e:
            sys.stderr.write(f"cros-fprint-bridge: DeleteEnrolledFingers error: {e}\n")

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='', out_signature='')
    def DeleteEnrolledFingers2(self):
        user = self.claimed_user or 'carrster'
        self.DeleteEnrolledFingers(user)

    @dbus.service.method(dbus.PROPERTIES_IFACE, in_signature='ss', out_signature='v')
    def Get(self, interface, prop):
        if prop == 'scan-type':
            return dbus.String('press')
        elif prop == 'name':
            return dbus.String('HP Chromebook x360 Fingerprint Sensor')
        elif prop == 'num-enroll-stages':
            return dbus.Int32(5)
        elif prop == 'finger-present':
            return dbus.Boolean(False)
        elif prop == 'finger-needed':
            return dbus.Boolean(False)
        raise dbus.exceptions.DBusException('Unknown property: ' + prop)

    @dbus.service.method(dbus.PROPERTIES_IFACE, in_signature='s', out_signature='a{sv}')
    def GetAll(self, interface):
        return {
            'scan-type': dbus.String('press'),
            'name': dbus.String('HP Chromebook x360 Fingerprint Sensor'),
            'num-enroll-stages': dbus.Int32(5),
            'finger-present': dbus.Boolean(False),
            'finger-needed': dbus.Boolean(False)
        }


class FprintManager(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, '/net/reactivated/Fprint/Manager')

    @dbus.service.method('net.reactivated.Fprint.Manager', in_signature='', out_signature='o')
    def GetDefaultDevice(self):
        return dbus.ObjectPath('/net/reactivated/Fprint/Device/0')

    @dbus.service.method('net.reactivated.Fprint.Manager', in_signature='', out_signature='ao')
    def GetDevices(self):
        return [dbus.ObjectPath('/net/reactivated/Fprint/Device/0')]


def main():
    try:
        bus = dbus.SystemBus()
    except Exception as e:
        sys.stderr.write(f"cros-fprint-bridge: Failed to connect to system bus: {e}\n")
        sys.exit(1)

    try:
        _name = dbus.service.BusName('net.reactivated.Fprint', bus)
    except Exception as e:
        sys.stderr.write(f"cros-fprint-bridge: Failed to claim net.reactivated.Fprint: {e}\n")
        sys.exit(1)

    _dev = FprintDevice(bus)
    _mgr = FprintManager(bus)

    loop = GLib.MainLoop()
    try:
        loop.run()
    except KeyboardInterrupt:
        pass


if __name__ == '__main__':
    main()
