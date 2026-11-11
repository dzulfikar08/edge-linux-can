#!/usr/bin/env python3
"""Host-side CAN peer for the QEMU smoke test, using only the Python stdlib.

Listens on a host SocketCAN interface (vcan0) with CAN FD frames enabled,
counts frames per id, and sends every 500 ms a classic frame with --send-id
("HOST" + 32-bit counter) and every second a 64-byte CAN FD frame with
--send-fd-id ("HOSTFD01", counter, counting pattern), so the guest sees both
kinds from the host. Frames listed in the DBC are decoded; the guest's FD
telemetry is checked byte for byte. Stops on SIGTERM/SIGINT or after
--seconds, then writes a JSON summary to --out.
"""

import argparse
import json
import os
import signal
import socket
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from minidbc import Dbc  # noqa: E402

CLASSIC = "=IB3x8s"   # struct can_frame: can_id, len, pad/res0/len8_dlc, data[8]
FD = "=IBB2x64s"      # struct canfd_frame: can_id, len, flags, res0/res1, data[64]
CAN_MTU, CANFD_MTU = 16, 72
CANFD_BRS = 0x01
EFF_FLAG, EFF_MASK, SFF_MASK = 0x80000000, 0x1FFFFFFF, 0x7FF


def telemetry_intact(data):
    """can-demo's FD telemetry: 'FDT1' at 4..8, bytes 32..63 = seq + i."""
    if len(data) != 64 or data[4:8] != b"FDT1":
        return None
    seq = data[0]
    return all(data[32 + i] == (seq + i) & 0xFF for i in range(32))


def host_fd_payload(n):
    head = b"HOSTFD01" + struct.pack("<I", n)
    return head + bytes((n + i) & 0xFF for i in range(64 - len(head)))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--iface", default="vcan0")
    ap.add_argument("--send-id", default="321")
    ap.add_argument("--send-fd-id", default="322", help="'none' to send no FD frames")
    ap.add_argument("--seconds", type=float, default=3600)
    ap.add_argument("--dbc", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "docs", "can-demo.dbc"))
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    dbc = Dbc(args.dbc)
    s = socket.socket(socket.AF_CAN, socket.SOCK_RAW, socket.CAN_RAW)
    s.setsockopt(socket.SOL_CAN_RAW, socket.CAN_RAW_FD_FRAMES, 1)
    s.bind((args.iface,))
    s.settimeout(0.1)
    send_id = int(args.send_id, 16)
    send_fd_id = None if args.send_fd_id == "none" else int(args.send_fd_id, 16)

    stop = False

    def on_signal(*_):
        nonlocal stop
        stop = True

    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGINT, on_signal)

    counts, fd_counts, first_seen, decoded = {}, {}, {}, {}
    telemetry = {"intact": 0, "damaged": 0}
    sent = sent_fd = 0
    start = time.monotonic()
    next_tx = start
    while not stop and time.monotonic() - start < args.seconds:
        now = time.monotonic()
        if now >= next_tx:
            s.send(struct.pack(CLASSIC, send_id, 8, b"HOST" + struct.pack("<I", sent)))
            if send_fd_id is not None and sent % 2 == 0:
                s.send(struct.pack(FD, send_fd_id, 64, CANFD_BRS, host_fd_payload(sent_fd)))
                sent_fd += 1
            sent += 1
            next_tx = now + 0.5
        try:
            raw = s.recv(CANFD_MTU)
        except socket.timeout:
            continue
        if len(raw) == CANFD_MTU:
            can_id, length, flags, data = struct.unpack(FD, raw)
            is_fd = True
        elif len(raw) == CAN_MTU:
            (can_id, length, data), flags, is_fd = struct.unpack(CLASSIC, raw), 0, False
        else:
            continue
        data = data[:length]
        cid = can_id & EFF_MASK if can_id & EFF_FLAG else can_id & SFF_MASK
        key = f"{cid:03X}"
        counts[key] = counts.get(key, 0) + 1
        if is_fd:
            fd_counts[key] = fd_counts.get(key, 0) + 1
            ok = telemetry_intact(data)
            if ok is not None:
                telemetry["intact" if ok else "damaged"] += 1
        first_seen.setdefault(key, {"len": length, "fd": is_fd, "brs": bool(flags & CANFD_BRS), "data": data.hex()})
        name, signals = dbc.decode(cid, data)
        if name:
            decoded[key] = {"message": name, "signals": signals}  # the last one seen

    with open(args.out, "w") as f:
        json.dump({"iface": args.iface, "sent_id": f"{send_id:03X}", "sent": sent,
                   "sent_fd_id": None if send_fd_id is None else f"{send_fd_id:03X}", "sent_fd": sent_fd,
                   "received": counts, "received_fd": fd_counts, "telemetry": telemetry,
                   "first": first_seen, "decoded": decoded,
                   "seconds": round(time.monotonic() - start, 1)}, f, indent=2)


if __name__ == "__main__":
    main()
