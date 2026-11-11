#!/usr/bin/env python3
"""Make two bundles the device must refuse, from a good verity bundle.

A RAUC bundle file is  [squashfs payload][CMS signature][signature size, 8 bytes BE].
For the verity format the signature is "inline": the CMS structure contains
the manifest (compatible, version, image hashes, dm-verity root hash) and
signs it. So a bundle can be re-signed, or its manifest changed, without
touching (or mounting) the payload:

  foreign-signed.raucb    same manifest, signed by keys/dev/other-ca, which
                          the device does not trust
  wrong-compatible.raucb  compatible=some-other-product, signed with OUR
                          development key: a valid signature for a bundle
                          meant for another product

Used by scripts/qemu-ab.sh (runs on the host, needs only openssl).
  forge-bundles.py GOOD.raucb KEYS_DIR OUT_DIR
"""

import os
import struct
import subprocess
import sys
import tempfile


def split(bundle):
    data = open(bundle, "rb").read()
    (size,) = struct.unpack(">Q", data[-8:])
    return data[: -8 - size], data[-8 - size: -8]


def manifest_of(sig):
    with tempfile.NamedTemporaryFile() as f:
        f.write(sig)
        f.flush()
        # -noverify: only extract the content here; the device does the verifying.
        return subprocess.run(["openssl", "cms", "-verify", "-noverify", "-binary",
                               "-inform", "DER", "-in", f.name],
                              check=True, capture_output=True).stdout


def sign(manifest, cert, key):
    with tempfile.NamedTemporaryFile() as f:
        f.write(manifest)
        f.flush()
        return subprocess.run(["openssl", "cms", "-sign", "-binary", "-nodetach", "-nosmimecap",
                               "-in", f.name, "-signer", cert, "-inkey", key, "-outform", "DER"],
                              check=True, capture_output=True).stdout


def write(path, payload, sig):
    with open(path, "wb") as f:
        f.write(payload + sig + struct.pack(">Q", len(sig)))
    print(f"wrote {path}")


def main():
    good, keys, out = sys.argv[1:4]
    os.makedirs(out, exist_ok=True)
    payload, sig = split(good)
    manifest = manifest_of(sig)
    if b"compatible=" not in manifest:
        sys.exit("no manifest found in the bundle's signature")

    write(f"{out}/foreign-signed.raucb", payload,
          sign(manifest, f"{keys}/other-ca/signing.cert.pem", f"{keys}/other-ca/signing.key.pem"))

    lines = [b"compatible=some-other-product" if line.startswith(b"compatible=") else line
             for line in manifest.split(b"\n")]
    write(f"{out}/wrong-compatible.raucb", payload,
          sign(b"\n".join(lines), f"{keys}/signing.cert.pem", f"{keys}/signing.key.pem"))


if __name__ == "__main__":
    main()
