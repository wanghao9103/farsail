"""Drag an isolated native viewer's outer frame under Xvfb/Openbox and measure it."""

import os
from pathlib import Path
import subprocess
import tempfile
import time

from Xlib import X, display, protocol
from Xlib.ext import xtest


def check_drags(connection, window):
    root = connection.screen().root

    def request(name, values):
        event = protocol.event.ClientMessage(
            window=window, client_type=connection.intern_atom(name), data=(32, values)
        )
        root.send_event(event, event_mask=X.SubstructureRedirectMask | X.SubstructureNotifyMask)
        connection.sync()

    def geometry():
        size = window.get_geometry()
        origin = root.translate_coords(window, 0, 0)
        return origin.x, origin.y, size.width, size.height

    def move(x, y):
        xtest.fake_input(connection, X.MotionNotify, x=x, y=y)
        connection.sync()

    request("_NET_ACTIVE_WINDOW", [2, 0, 0, 0, 0])
    for inset, sign in [(1, 1), (7, 1), (7, -1)]:
        for edge in ["E", "W", "S", "N", "SE", "NW", "NE", "SW"]:
            # EWMH flags select x, y, width and height; Openbox restores the same baseline.
            request(
                "_NET_MOVERESIZE_WINDOW",
                [(15 << 8) | X.StaticGravity | (2 << 12), 160, 100, 700, 500],
            )
            time.sleep(0.15)
            x, y, w, h = geometry()
            px = x + (inset if "W" in edge else w - 1 - inset if "E" in edge else w // 2)
            py = y + (inset if "N" in edge else h - 1 - inset if "S" in edge else h // 2)
            dx = sign * (48 if "E" in edge else -48 if "W" in edge else 0)
            dy = sign * (36 if "S" in edge else -36 if "N" in edge else 0)
            move(px, py)
            time.sleep(0.06)
            xtest.fake_input(connection, X.ButtonPress, 1)
            connection.sync()
            time.sleep(0.12)
            for step in range(1, 9):
                move(px + dx * step // 8, py + dy * step // 8)
                time.sleep(0.02)
            xtest.fake_input(connection, X.ButtonRelease, 1)
            connection.sync()
            expected = (
                x + (dx if "W" in edge else 0), y + (dy if "N" in edge else 0),
                w + sign * abs(dx), h + sign * abs(dy),
            )
            deadline = time.monotonic() + 2
            while any(abs(a - b) > 1 for a, b in zip(geometry(), expected)):
                if time.monotonic() >= deadline:
                    raise AssertionError(
                        f"{edge} inset={inset} delta=({dx},{dy}): expected {expected}, got {geometry()}"
                    )
                time.sleep(0.05)
    print("PASS native viewer: 24 consecutive edge/corner drags, including inward resize")


def main():
    with tempfile.TemporaryDirectory(dir=os.environ["XDG_RUNTIME_DIR"]) as temp:
        report = Path(temp) / "viewer.json"
        env = dict(
            os.environ, FARSAIL_VIEWER_SMOKE="1", FARSAIL_IPC_SMOKE_PATH=str(report),
            FARSAIL_TEST_PROFILE_DIR=str(Path(temp) / "profile"),
        )
        env.pop("FARSAIL_UBUNTU_SMOKE", None)
        app = subprocess.Popen([env["FARSAIL_TEST_BINARY"]], env=env)
        try:
            deadline = time.monotonic() + 60
            while not Path(str(report) + ".closed").exists():
                if app.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError(report.read_text() if report.exists() else "Viewer fixture did not open")
                time.sleep(0.1)
            connection = display.Display()
            clients = connection.screen().root.get_full_property(
                connection.intern_atom("_NET_CLIENT_LIST"), X.AnyPropertyType
            )
            windows = (connection.create_resource_object("window", wid) for wid in clients.value)
            window = next(w for w in windows if w.get_wm_name() == "FarSail cascade fixture")
            check_drags(connection, window)
        finally:
            app.terminate()
            app.wait(timeout=10)


if __name__ == "__main__":
    main()
