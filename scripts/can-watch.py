#!/usr/bin/env python3
"""Print live CAN traffic from a SocketCAN interface, decoded with DBC files.

    scripts/can-watch.py vcan0 docs/can-demo.dbc [--secs 20] [--only 101,321]

Standard library only (decoding by minidbc.py). candump shows the raw bytes;
this adds the message and signal names. Classic and CAN FD frames: the
socket enables CAN_RAW_FD_FRAMES, so reads return 16 bytes (can_frame) or
72 bytes (canfd_frame); FD frames are marked "FD" (and "BRS" with bit rate
switch) and long payloads are shortened on screen.
"""
import argparse
import socket
import struct
import sys
import time
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from minidbc import Dbc  # noqa: E402

FMT = "=IB3x8s"     # struct can_frame, 16 bytes
FD_FMT = "=IBB2x64s"  # struct canfd_frame, 72 bytes
CANFD_BRS = 0x01
EFF_FLAG, ERR_FLAG, RTR_FLAG = 0x80000000, 0x20000000, 0x40000000


def main():
    ap = argparse.ArgumentParser(description="Decode live SocketCAN traffic with DBC files")
    ap.add_argument("ifname", nargs="?", default="vcan0")
    ap.add_argument("dbc", nargs="*", help="DBC files (Intel byte order only)")
    ap.add_argument("--secs", type=float, default=10)
    ap.add_argument("--only", default="", help="comma-separated hex ids to print (all are counted)")
    a = ap.parse_args()

    dbcs = [Dbc(p) for p in a.dbc]
    only = {int(x, 16) for x in a.only.split(",") if x}
    s = socket.socket(socket.AF_CAN, socket.SOCK_RAW, socket.CAN_RAW)
    s.setsockopt(socket.SOL_CAN_RAW, socket.CAN_RAW_FD_FRAMES, 1)
    s.bind((a.ifname,))
    s.settimeout(0.2)
    n_msgs = sum(len(d.by_id) for d in dbcs)
    print(f"can-watch: {a.ifname} for {a.secs:g}s, {len(dbcs)} DBC file(s), {n_msgs} messages", flush=True)

    counts, t0 = Counter(), time.monotonic()
    try:
        while time.monotonic() - t0 < a.secs:
            try:
                raw = s.recv(72)
            except socket.timeout:
                continue
            if len(raw) == 72:
                raw_id, dlc, flags, data = struct.unpack(FD_FMT, raw)
                kind = "FD BRS" if flags & CANFD_BRS else "FD"
            else:
                raw_id, dlc, data = struct.unpack(FMT, raw)
                kind = ""
            if raw_id & (ERR_FLAG | RTR_FLAG):
                continue
            fid = raw_id & (0x1FFFFFFF if raw_id & EFF_FLAG else 0x7FF)
            data = data[:dlc]
            counts[fid] += 1
            if only and fid not in only:
                continue
            shown_bytes = data.hex(' ').upper() if dlc <= 8 else data[:8].hex(' ').upper() + " …"
            line = f"{time.monotonic() - t0:>10.3f}  {fid:03X}  [{dlc:>2}] {kind:<6} {shown_bytes:<25}"
            for d in dbcs:
                name, sig = d.decode(fid, data)
                if name:
                    shown = [f"{k}={sig.get(k + '_label', v)}" for k, v in sig.items() if not k.endswith("_label")]
                    line += f"  {name} {' '.join(shown)}"
                    break
            print(line, flush=True)
    except KeyboardInterrupt:
        pass
    print("summary: " + " ".join(f"{i:03X}={n}" for i, n in sorted(counts.items())))


if __name__ == "__main__":
    main()
