# Backport of zlib's fix for a heap buffer overflow in gzwrite() after a
# failed write (CVE-2026-85091, CVE-2026-76844); in zlib 1.4, which oe-core
# master has and wrynose (1.3.2) does not. The patch's CVE: line marks both
# as patched in the SBOM/VEX. Drop when zlib is upgraded.
# Target only: zlib-native is a build tool that does not ship, and patching
# it would change the inputs of every native recipe that links it (the Rust
# and GCC toolchains, the kernel's host tools) and rebuild them all.
FILESEXTRAPATHS:prepend := "${THISDIR}/files:"
SRC_URI:append:class-target = " file://0001-Fix-buffer-overflow-bug-in-non-blocking-gzwrite.patch"
