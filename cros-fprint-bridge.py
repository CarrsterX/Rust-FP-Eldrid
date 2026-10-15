#!/usr/bin/env python3
"""
ChromeOS FPMCU to Fprint D-Bus Bridge for GNOME Shell / GDM
Emulates net.reactivated.Fprint on the system bus so GNOME Shell / GDM
enables parallel gdm-fingerprint and gdm-password authentication workers.
"""

import sys
import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

DBusGMainLoop(set_as_default=True)

class FprintDevice(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, '/net/reactivated/Fprint/Device/0')

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='s', out_signature='as')
    def ListEnrolledFingers(self, username):
        return ['any']

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='s', out_signature='')
    def Claim(self, username):
        pass

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='', out_signature='')
    def Release(self):
        pass

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='s', out_signature='')
    def VerifyStart(self, finger_name):
        pass

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='', out_signature='')
    def VerifyStop(self):
        pass

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='s', out_signature='')
    def EnrollStart(self, finger_name):
        pass

    @dbus.service.method('net.reactivated.Fprint.Device', in_signature='', out_signature='')
    def EnrollStop(self):
        pass

    @dbus.service.method(dbus.PROPERTIES_IFACE, in_signature='ss', out_signature='v')
    def Get(self, interface, prop):
        if prop == 'scan-type':
            return dbus.String('press')
        elif prop == 'name':
            return dbus.String('HP Chromebook x360 Fingerprint Sensor')
        elif prop == 'num-enroll-stages':
            return dbus.Int32(5)
        raise dbus.exceptions.DBusException('Unknown property: ' + prop)

    @dbus.service.method(dbus.PROPERTIES_IFACE, in_signature='s', out_signature='a{sv}')
    def GetAll(self, interface):
        return {
            'scan-type': dbus.String('press'),
            'name': dbus.String('HP Chromebook x360 Fingerprint Sensor'),
            'num-enroll-stages': dbus.Int32(5)
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
