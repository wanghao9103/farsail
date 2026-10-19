#!/usr/bin/python3
"""Integration-only StatusNotifier host and exported menu inspection, on the isolated test bus."""
import json
import os
import sys
import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

if sys.argv[1] == 'host':
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    name = dbus.service.BusName('org.kde.StatusNotifierWatcher', bus)

    class Watcher(dbus.service.Object):
        items = []

        @dbus.service.method('org.kde.StatusNotifierWatcher', in_signature='s', sender_keyword='sender')
        def RegisterStatusNotifierItem(self, service, sender=None):
            item = str(sender) + str(service) if str(service).startswith('/') else str(service) + '/StatusNotifierItem'
            self.items.append(item)
            self.StatusNotifierItemRegistered(item)

        @dbus.service.method('org.kde.StatusNotifierWatcher', in_signature='s')
        def RegisterStatusNotifierHost(self, host):
            pass

        @dbus.service.signal('org.kde.StatusNotifierWatcher', signature='s')
        def StatusNotifierItemRegistered(self, item):
            pass

        @dbus.service.method('org.freedesktop.DBus.Properties', in_signature='ss', out_signature='v')
        def Get(self, interface, prop):
            return self.GetAll(interface)[prop]

        @dbus.service.method('org.freedesktop.DBus.Properties', in_signature='s', out_signature='a{sv}')
        def GetAll(self, interface):
            return {'RegisteredStatusNotifierItems': dbus.Array(self.items, signature='s'), 'IsStatusNotifierHostRegistered': dbus.Boolean(True), 'ProtocolVersion': dbus.Int32(0)}

    watcher = Watcher(bus, '/StatusNotifierWatcher')
    open(os.path.join(os.environ['XDG_RUNTIME_DIR'], 'tray-host-ready'), 'w').close()
    GLib.MainLoop().run()
else:
    bus = dbus.SessionBus()
    host = dbus.Interface(bus.get_object('org.kde.StatusNotifierWatcher', '/StatusNotifierWatcher'), 'org.freedesktop.DBus.Properties')
    items = host.Get('org.kde.StatusNotifierWatcher', 'RegisteredStatusNotifierItems')
    assert len(items) == 1, f'Expected one tray icon, got {len(items)}'
    service, path = str(items[0]).split('/', 1)
    props = dbus.Interface(bus.get_object(service, '/' + path), 'org.freedesktop.DBus.Properties')
    menu_path = props.Get('org.kde.StatusNotifierItem', 'Menu')
    menu = dbus.Interface(bus.get_object(service, menu_path), 'com.canonical.dbusmenu')
    _, layout = menu.GetLayout(0, -1, [])
    entries = [child for child in layout[2] if 'label' in child[1]]
    labels = [str(child[1]['label']) for child in entries]
    assert all(label in labels for label in ['打开 FarSail', '退出 FarSail']), labels
    print(json.dumps({'trayMenu': True, 'singleTray': True, 'labels': labels}, ensure_ascii=False))

    if sys.argv[1] in ('open', 'quit'):
        label = '打开 FarSail' if sys.argv[1] == 'open' else '退出 FarSail'
        item = next(child for child in entries if str(child[1]['label']) == label)
        menu.Event(item[0], 'clicked', dbus.Int32(0), dbus.UInt32(0))
