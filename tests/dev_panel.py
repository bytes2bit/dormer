#!/usr/bin/env python3
"""Exercise real TUI input through a PTY with isolated, offline runtime state."""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import sys
import tempfile
import termios
import time


def main():
    binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else "target/release/dormer").resolve())
    with tempfile.TemporaryDirectory(prefix="dormer-panel-") as directory:
        root = Path(directory)
        config = root / "config/dormer"
        config.mkdir(parents=True)
        (config / "config.toml").write_text('''[look]
view = "rooftops"
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
''')
        env = os.environ.copy()
        env.pop("HYPRLAND_INSTANCE_SIGNATURE", None)
        env.update(XDG_CONFIG_HOME=str(root / "config"), XDG_CACHE_HOME=str(root / "cache"),
                   XDG_STATE_HOME=str(root / "state"), XDG_RUNTIME_DIR=str(root), TZ="UTC")
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 70, 0, 0))
        process = subprocess.Popen([binary, "dev", "--here"], stdin=slave, stdout=slave, stderr=slave, env=env)
        os.close(slave)
        transcript = bytearray()

        def drain(seconds=0.15):
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                if select.select([master], [], [], min(0.02, max(0, end - time.monotonic())))[0]:
                    try:
                        transcript.extend(os.read(master, 65536))
                    except OSError:
                        break

        def send(keys):
            os.write(master, keys)
            drain()

        state = root / "cache/dormer/dev.json"
        try:
            deadline = time.monotonic() + 5
            while not state.exists() and time.monotonic() < deadline:
                drain()
            assert state.exists(), "panel did not publish its initial state"
            send(b"\x1b[C")
            latest = transcript.rsplit(b"\x1b[H\x1b[2J", 1)[-1]
            assert b"time of day" in latest and b"preview" in latest, "first adjustment must show its selected control"
            send(b"2\x1b[C")
            first = json.loads(state.read_text())["set"]["cover"]
            assert first > 0
            drain(1.1)  # let the keyboard-adjustment preview return before clicking a full-panel row
            send(b"\x1b[<0;2")
            send(b"0;8M\x1b[<32;100;99M\x1b[<0;100;99m")
            assert json.loads(state.read_text())["set"]["cover"] == 100
            assert b"preview" in transcript
            send(b"5a")  # empty Scene group must still allow global reset
            assert json.loads(state.read_text())["set"] == {}
            stamp = state.stat().st_mtime_ns
            def idle_switches():
                total = 0
                for path in Path(f"/proc/{process.pid}/task").glob("*/status"):
                    for line in path.read_text().splitlines():
                        if line.startswith("voluntary_ctxt_switches:"):
                            total += int(line.split()[1])
                return total
            before = idle_switches()
            drain(0.3)
            assert idle_switches() - before <= 3, "idle painter polls instead of sleeping until a change"
            assert state.stat().st_mtime_ns == stamp, "idle panel rewrites overrides"
            send(b"q")
            assert process.wait(timeout=5) == 0
            assert not state.exists(), "quit must restore reality and remove overrides"
            assert b"\x1b[?1049l" in transcript, "alternate screen was not restored"
            print("Dev PTY: fragmented input, drag/release, compact preview, reset, idle and cleanup passed.")
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
            os.close(master)


if __name__ == "__main__":
    main()
