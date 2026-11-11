#!/usr/bin/env python3
"""Host-side CAN peer for the QEMU smoke test, using only the Python stdlib.

Listens on a host SocketCAN interface (vcan0), counts frames per id, and sends
a frame with --send-id every 500 ms ("HOST" + 32-bit counter) so the guest can
see host traffic. Stops on SIGTERM/SIGINT or after --seconds, then writes a
JSON summary to --out.
"""

import argparse
import json
import signal
import socket
import struct
import time

FMT = "=IB3x8s"  # struct can_frame: can_id, len, pad/res0/len8_dlc, data[8]
EFF_FLAG, EFF_MASK, SFF_MASK = 0x80000000, 0x1FFFFFFF, 0x7FF


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--iface", default="vcan0")
    ap.add_argument("--send-id", default="321")
    ap.add_argument("--seconds", type=float, default=3600)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    s = socket.socket(socket.AF_CAN, socket.SOCK_RAW, socket.CAN_RAW)
    s.bind((args.iface,))
    s.settimeout(0.1)
    send_id = int(args.send_id, 16)

    stop = False

    def on_signal(*_):
        nonlocal stop
        stop = True

    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGINT, on_signal)

    counts, first_seen, sent = {}, {}, 0
    start = time.monotonic()
    next_tx = start
    while not stop and time.monotonic() - start < args.seconds:
        now = time.monotonic()
        if now >= next_tx:
            s.send(struct.pack(FMT, send_id, 8, b"HOST" + struct.pack("<I", sent)))
            sent += 1
            next_tx = now + 0.5
        try:
            raw = s.recv(16)
        except socket.timeout:
            continue
        can_id, length, data = struct.unpack(FMT, raw)
        cid = can_id & EFF_MASK if can_id & EFF_FLAG else can_id & SFF_MASK
        key = f"{cid:03X}"
        counts[key] = counts.get(key, 0) + 1
        first_seen.setdefault(key, data[:length].hex())

    with open(args.out, "w") as f:
        json.dump({"iface": args.iface, "sent_id": f"{send_id:03X}", "sent": sent,
                   "received": counts, "first_payload": first_seen,
                   "seconds": round(time.monotonic() - start, 1)}, f, indent=2)


if __name__ == "__main__":
    main()
