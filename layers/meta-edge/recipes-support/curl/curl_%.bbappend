# CVE triage for curl 8.19.0 (docs/cve/README.md).
#
# This build of libcurl (PACKAGECONFIG from oe-core + meta-rauc) has, per
# /usr/lib/pkgconfig/libcurl.pc in the sysroot and `curl-config`:
#   ssl backend:        OpenSSL only
#   supported_protocols="FILE FTP FTPS HTTP HTTPS IPFS IPNS"
#   supported_features="alt-svc AsynchDNS HSTS HTTP2 HTTPS-proxy IDN IPv6
#                        Largefile libz NTLM SSL threadsafe TLS-SRP UnixSockets"
# The entries below are CVEs whose advisory on curl.se limits them to a TLS
# backend, protocol or feature that is not in that list. If PACKAGECONFIG
# changes (e.g. krb5, libssh2, gnutls, wolfssl, websockets), re-check them.
CVE_STATUS[CVE-2026-87866] = "not-applicable-config: wolfSSL backend only (https://curl.se/docs/CVE-2026-87866.html); built with OpenSSL"
CVE_STATUS[CVE-2026-82208] = "not-applicable-config: wolfSSL backend only (https://curl.se/docs/CVE-2026-82208.html); built with OpenSSL"
CVE_STATUS[CVE-2026-107928] = "not-applicable-config: WebSockets with the GnuTLS backend (https://curl.se/docs/CVE-2026-107928.html); built with OpenSSL, no WebSockets"
CVE_STATUS[CVE-2026-87864] = "not-applicable-config: IMAP (https://curl.se/docs/CVE-2026-87864.html); IMAP not among supported_protocols"
CVE_STATUS[CVE-2026-92389] = "not-applicable-config: TFTP (https://curl.se/docs/CVE-2026-92389.html); TFTP not among supported_protocols"
CVE_STATUS[CVE-2026-86410] = "not-applicable-config: redirect to SFTP/SCP (https://curl.se/docs/CVE-2026-86410.html); built without libssh2, SCP/SFTP not supported"
CVE_STATUS[CVE-2026-87865] = "not-applicable-config: libssh2 known_hosts handling (https://curl.se/docs/CVE-2026-87865.html); built without libssh2"
CVE_STATUS[CVE-2026-19931] = "not-applicable-config: Negotiate (SPNEGO) authentication (https://curl.se/docs/CVE-2026-19931.html) needs GSS-API; built without krb5/GSS-API, no SPNEGO in supported_features"
