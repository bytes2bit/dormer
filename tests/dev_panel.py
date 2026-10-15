#!/usr/bin/env python3
"""Drive the settings panel through a real PTY, with isolated offline runtime state.

Covers: fragmented keyboard/mouse input, stable dragging, direct entry, fixed
source labels, keyboard and mouse Save, Discard, quit Save/Discard/Cancel,
conflicting external edits, save failure, comment preservation, persistence
after relaunch, resize, idle wakeups and cleanup.

    python3 tests/dev_panel.py [path/to/dormer]
"""
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import time

BINARY = None
CONFIG = '''# my dormer
[look]
view = "rooftops"   # which place
mode = "dark"
size = "320x200"
[weather]
source = "none"
[house]
lamp = []
containers = false
[wallpaper]
backend = "command"
command = "true"
[unrelated]
note = "keep me"
'''

CELL = re.compile(rb"\x1b\[(\d+);(\d+)H((?:\x1b\[[\d;?]*[mK])*)([^\x1b]*)")


class Panel:
    """One run of `dormer dev --here` on a PTY."""

    def __init__(self, root, rows=24, cols=70, config=CONFIG, name="panel"):
        self.root = Path(root)
        cfg = self.root / "config/dormer"
        cfg.mkdir(parents=True, exist_ok=True)
        self.config = cfg / "config.toml"
        if config is not None:
            self.config.write_text(config)
        self.dev = self.root / "cache/dormer/dev.json"
        env = os.environ.copy()
        env.pop("HYPRLAND_INSTANCE_SIGNATURE", None)
        env.update(XDG_CONFIG_HOME=str(self.root / "config"), XDG_CACHE_HOME=str(self.root / "cache"),
                   XDG_STATE_HOME=str(self.root / "state"), XDG_RUNTIME_DIR=str(self.root), TZ="UTC")
        self.master, slave = pty.openpty()
        self.resize(rows, cols)
        self.process = subprocess.Popen([BINARY, "dev", "--here"], stdin=slave, stdout=slave, stderr=slave, env=env)
        os.close(slave)
        self.transcript = bytearray()
        self.drain(0.6)

    def resize(self, rows, cols):
        fcntl.ioctl(self.master, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        if getattr(self, "process", None) is not None:
            self.process.send_signal(28)  # SIGWINCH

    def drain(self, seconds=0.2):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if select.select([self.master], [], [], min(0.02, max(0, end - time.monotonic())))[0]:
                try:
                    self.transcript.extend(os.read(self.master, 65536))
                except OSError:
                    break

    def send(self, data, wait=0.2):
        os.write(self.master, data if isinstance(data, bytes) else data.encode())
        self.drain(wait)

    def frame(self):
        """The last complete frame as {(row, col): text}, 1-based like the terminal."""
        start = self.transcript.rfind(b"\x1b[1;1H\x1b[2K")
        cells = {}
        for m in CELL.finditer(bytes(self.transcript[start:])):
            text = m.group(4).decode("utf-8", "replace")
            if text:
                cells[(int(m.group(1)), int(m.group(2)))] = text
        return cells

    def text(self):
        rows = {}
        for (r, c), t in sorted(self.frame().items()):
            rows.setdefault(r, []).append((c, t))
        return "\n".join("".join(t for _, t in sorted(v)) for _, v in sorted(rows.items()))

    def where(self, needle):
        """(col, row) of a piece of text in the last frame, 0-based."""
        for (r, c), t in self.frame().items():
            i = t.find(needle)
            if i >= 0:
                return c - 1 + i, r - 1
        raise AssertionError(f"{needle!r} is not on screen:\n{self.text()}")

    def has(self, needle):
        return needle in self.text()

    def press(self, x, y, wait=0.2):
        self.send(f"\x1b[<0;{x + 1};{y + 1}M", wait)

    def drag(self, x, y, wait=0.2):
        self.send(f"\x1b[<32;{x + 1};{y + 1}M", wait)

    def release(self, x, y, wait=0.25):
        self.send(f"\x1b[<0;{x + 1};{y + 1}m", wait)

    def click(self, x, y):
        self.press(x, y)
        self.release(x, y)

    def held(self):
        return json.loads(self.dev.read_text())["set"] if self.dev.exists() else {}

    def wait_for(self, check, what, seconds=4.0):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            self.drain(0.05)
            try:
                if check():
                    return
            except (AssertionError, KeyError, ValueError):
                pass
        raise AssertionError(f"timed out waiting for {what}:\n{self.text()}")

    def config_text(self):
        return self.config.read_text()

    def idle_switches(self):
        total = 0
        for path in Path(f"/proc/{self.process.pid}/task").glob("*/status"):
            for line in path.read_text().splitlines():
                if line.startswith("voluntary_ctxt_switches:"):
                    total += int(line.split()[1])
        return total

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
            self.process.wait(timeout=5)
        os.close(self.master)


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def scenario(name):
    def wrap(fn):
        def run():
            with tempfile.TemporaryDirectory(prefix="dp-") as d:
                fn(Path(d))
            print(f"  ok  {name}")
        run.__name__ = fn.__name__
        return run
    return wrap


@scenario("first adjustment, fragmented mouse input, drag, idle, reset and cleanup")
def basics(root):
    p = Panel(root)
    try:
        check(p.has("Saved") and p.has("time of day") and p.has("[auto]"), "starts saved, with time automatic:\n" + p.text())
        p.send(b"\x1b[C")
        check("time" in p.held(), "first adjustment must publish the fixed time")
        check(p.has("[fixed]") and p.has("preview"), "the adjusted control shows Fixed and the preview state:\n" + p.text())
        p.drain(1.2)  # the preview returns by itself
        check(p.has("Unsaved changes (1)"), "state in words:\n" + p.text())
        # move to the weather and press on the cloud slider with the report split in three
        for _ in range(2):
            p.send(b"j")
        x, y = p.where("cloud")
        check(p.has("cloud"), "cloud is listed")
        bar_x = x + 18
        p.send(b"\x1b[<0;")
        p.send(f"{bar_x + 1};{y + 1}M")
        first = p.held()["cover"]
        p.send(f"\x1b[<32;{bar_x + 60};{y + 1}M")  # dragged far right, and off the row
        p.send(f"\x1b[<0;{bar_x + 60};{y + 8}m")  # released on another row entirely
        check(p.held()["cover"] == 100, f"drag must follow the slider even outside its row (was {first})")
        check(p.has("Unsaved changes"), "still unsaved: nothing was saved by the drag")
        check(not p.config_text().count("cloud"), "a drag must never write the config")
        p.send(b"a")  # give cloud back to reality
        check("cover" not in p.held(), "a returns the field to reality")
        before = p.idle_switches()
        p.drain(0.4)
        check(p.idle_switches() - before <= 3, "an idle panel polls instead of sleeping")
        stamp = p.dev.stat().st_mtime_ns
        p.drain(0.3)
        check(p.dev.stat().st_mtime_ns == stamp, "an idle panel rewrites its overrides")
        p.send(b"d")
        check(p.held() == {} and not p.dev.exists(), "Discard returns to the saved scene and drops the preview")
        p.send(b"q")
        check(p.process.wait(timeout=5) == 0, "a clean panel quits at once")
        check(not p.dev.exists(), "quit must remove the transient preview")
        check(b"\x1b[?1049l" in p.transcript, "alternate screen was not restored")
        check("[scene]" not in p.config_text(), "nothing was saved")
    finally:
        p.close()


@scenario("direct entry validates, applies, and Escape cancels")
def direct_entry(root):
    p = Panel(root)
    try:
        p.send(b"\r")  # Enter on the time
        p.send(b"21:30")
        p.send(b"\r")
        check(abs(p.held()["time"] - 21.5) < 1e-6, f"typed 21:30 should hold 21.5 hours: {p.held()}")
        p.send(b"\r")
        p.send(b"25:99\r")
        check(p.has("✗"), "an invalid time stays open with the reason:\n" + p.text())
        check(abs(p.held()["time"] - 21.5) < 1e-6, "nothing applied")
        p.send(b"\x1b", 0.3)  # a lone Escape cancels the entry
        check(not p.has("✗") and p.has("21:30"), "Escape closed the entry and kept the value:\n" + p.text())
        p.send(b"\r")
        p.send(b"auto\r")
        check("time" not in p.held(), "typing auto hands it back to reality")
        p.send(b"d")
    finally:
        p.close()


@scenario("keyboard Save writes the config, keeps comments, and persists across relaunch")
def keyboard_save(root):
    p = Panel(root)
    try:
        p.send(b"\r")
        p.send(b"21:30\r")
        p.send(b"j")  # date
        p.send(b"\x1b[C")
        p.send(b"s")
        p.wait_for(lambda: p.has("Saved to"), "the Saved message")
        text = p.config_text()
        check('time = "21:30"' in text, text)
        check("date = " in text and re.search(r'date = "\d{4}-\d\d-\d\d"', text), "the date is saved as an ISO date, not an offset:\n" + text)
        check("# my dormer" in text and "# which place" in text and 'note = "keep me"' in text, "comments and unrelated keys survive:\n" + text)
        check((p.config.parent / "config.toml.bak").read_text() == CONFIG, "the previous file is kept as a backup")
        check(not p.dev.exists(), "after Save the preview is just the saved scene")
        p.send(b"q")
        check(p.process.wait(timeout=5) == 0, "saved: quits at once")
    finally:
        p.close()
    # a new process: the saved values are the starting point, fixed
    p2 = Panel(root, config=None)
    try:
        check(p2.has("Saved") and p2.has("21:30") and p2.has("[fixed]"), "a relaunch starts from what was saved:\n" + p2.text())
        check(not p2.dev.exists(), "nothing is being previewed")
        p2.send(b"q")
        check(p2.process.wait(timeout=5) == 0, "")
    finally:
        p2.close()
    # and a plain export in a fresh process agrees
    env = os.environ.copy()
    env.pop("HYPRLAND_INSTANCE_SIGNATURE", None)
    env.update(XDG_CONFIG_HOME=str(root / "config"), XDG_CACHE_HOME=str(root / "cache"), XDG_STATE_HOME=str(root / "state"), TZ="UTC")
    out = subprocess.run([BINARY, "explain"], env=env, capture_output=True, text=True, timeout=30).stdout
    check("21:30" in out, "explain in a new process uses the saved fixed time:\n" + out)


@scenario("mouse Save needs press and release on the button; quitting unsaved asks")
def mouse_save_and_quit(root):
    p = Panel(root)
    try:
        p.send(b"\x1b[C")
        p.drain(1.2)
        x, y = p.where("[Save]")
        p.press(x + 1, y)
        check("[scene]" not in p.config_text(), "a press alone does not save")
        p.release(x + 1, y)
        p.wait_for(lambda: p.has("Saved to"), "Saved after press and release")
        check("[scene]" in p.config_text(), "the click saved")
        # press on Save but release elsewhere: nothing
        p.send(b"\x1b[C")
        p.drain(1.2)
        p.press(x + 1, y)
        p.release(2, 5)
        check(p.has("Unsaved changes"), "released away from the button: not saved:\n" + p.text())
        # quit with unsaved changes: Cancel, then Discard, then (again) Save
        p.send(b"q")
        check(p.has("unsaved changes") and p.has("Cancel"), "the quit question:\n" + p.text())
        p.send(b"c")
        check(p.process.poll() is None and p.has("Unsaved changes"), "Cancel keeps the panel and the edit")
        p.send(b"q")
        qx, qy = p.where("Discard")
        p.click(qx + 1, qy)
        check(p.process.wait(timeout=5) == 0, "Discard leaves")
        saved = p.config_text()
        check(saved.count("time =") == 1, "the discarded edit was not written:\n" + saved)
    finally:
        p.close()
    p = Panel(root, config=None)
    try:
        p.send(b"\x1b[C")
        p.send(b"q")
        p.send(b"s")
        check(p.process.wait(timeout=5) == 0, "Save from the question saves, then exits")
        check(p.config_text().count("time =") == 1, p.config_text())
    finally:
        p.close()


@scenario("an external edit while open is a conflict: reload or overwrite, never silent")
def conflict(root):
    p = Panel(root)
    try:
        p.send(b"\x1b[C")
        p.drain(1.0)
        external = CONFIG + "# edited elsewhere\n"
        p.config.write_text(external)
        p.send(b"s")
        check(p.has("changed by something else"), "the conflict is explained:\n" + p.text())
        check(p.config_text() == external, "the external edit is untouched")
        check(p.has("Overwrite") and p.has("Reload"), "reload and overwrite are offered")
        p.send(b"c")
        check(p.has("Not saved (1 unsaved)"), "cancel keeps the draft, and says it is not saved:\n" + p.text())
        p.send(b"s")
        p.send(b"o")
        p.wait_for(lambda: p.has("Saved to"), "overwrite saves")
        text = p.config_text()
        check("# edited elsewhere" in text and "[scene]" in text, "the external edit survives an explicit overwrite:\n" + text)
        p.send(b"q")
        p.process.wait(timeout=5)
    finally:
        p.close()


@scenario("a failed save says why and keeps the draft")
def failed_save(root):
    p = Panel(root)
    cfg_dir = p.config.parent
    try:
        p.send(b"\x1b[C")
        p.drain(1.0)
        os.chmod(cfg_dir, 0o500)  # no new files in the config directory
        if os.access(cfg_dir, os.W_OK):
            print("  --  (skipped: directory permissions are not enforced here)")
            return
        p.send(b"s")
        p.wait_for(lambda: p.has("Not saved"), "the failure")
        check(p.has("Not saved: cannot") and p.has("still here"), "actionable error, whole:\n" + p.text())
        check("[scene]" not in p.config_text(), "the file is untouched")
        p.send(b"q")
        check(p.has("unsaved changes"), "quit still asks")
        p.send(b"s")  # Save from the question fails too: must not exit
        p.drain(0.5)
        check(p.process.poll() is None, "a failed save must not close the panel")
        check(p.has("Not saved"), p.text())
        os.chmod(cfg_dir, 0o700)
        p.send(b"s")
        p.wait_for(lambda: p.has("Saved to"), "saving works once the directory is writable")
        p.send(b"q")
        p.process.wait(timeout=5)
    finally:
        os.chmod(cfg_dir, 0o700)
        p.close()


@scenario("small terminals keep Save, Discard and Help reachable, and resizing cancels a drag")
def small_and_resize(root):
    p = Panel(root, rows=12, cols=40)
    try:
        for label in ("[Save]", "[Discard]", "[Help]", "[Quit]"):
            p.where(label)
        # every drawn row fits the terminal
        for (r, c), t in p.frame().items():
            check(r <= 12 and c - 1 + len(t) <= 40 + 2, f"{t!r} at {(r, c)} spills past 40x12")
        # start a drag, resize mid-gesture, keep moving: the value stops following
        x, y = p.where("time of day")
        bar_x = x + 13
        p.press(bar_x, y)
        value = p.held()["time"]
        p.resize(24, 70)
        p.drain(0.3)
        p.drag(bar_x + 10, y)
        check(p.held()["time"] == value, "a resize must end the gesture")
        p.release(bar_x + 10, y)
        p.where("[Save]")
        p.send(b"d")
        p.send(b"q")
        p.process.wait(timeout=5)
    finally:
        p.close()


def main():
    global BINARY
    BINARY = str(Path(sys.argv[1] if len(sys.argv) > 1 else "target/release/dormer").resolve())
    print(f"settings panel PTY checks against {BINARY}")
    for run in (basics, direct_entry, keyboard_save, mouse_save_and_quit, conflict, failed_save, small_and_resize):
        run()
    print("Dev PTY: all workflows passed.")


if __name__ == "__main__":
    main()
