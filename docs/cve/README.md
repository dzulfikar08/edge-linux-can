# CVE triage for edge-image (2026-11-11/12)

What was done to the CVE report of the product image `edge-image`, why, and
what is left. The method is explained in
[learning-guide.md, chapter 32](../learning-guide.md#32-cve-triage-from-a-report-to-decisions);
what happened during the builds in
[build-journal.md, part 4](../build-journal.md#part-4-qemu-patch-headless-distro-layer-update-cve-triage).

Reports (cve-check JSON format from `sbom-cve-check`, CVE databases pinned to
2026-11-11 04:55 UTC, see `kas/qemuarm64.yml`):
[before](edge-image-summary-2026-11-11-before.md) · [after](edge-image-summary.md).
"Installed" means recipes with at least one package in the image manifest;
the SBOM also lists recipes that were only needed to build the image.

## Before and after

| | Before (yocto-6.0.4 pins, poky distro features) | After the update (layers, kernel, headless distro) | After triage (final) |
|---|---|---|---|
| Unpatched, installed recipes | **703** | 157 | **133** |
| Patched, installed recipes | 21,217 | 22,883 | 22,885 |
| Ignored, installed recipes | 1,258 | 118 | 140 |
| Installed recipes in the SBOM | 64 | 60 | 60 |

(Ignored went *down* with the update because most kernel CVEs that the
scanner had ignored as "source not compiled" are now simply fixed by version:
the scanner checks the version first and only looks at compiled sources for
CVEs it still considers affected.)

Per recipe (installed recipes with unpatched CVEs at any stage):

| Recipe | Version before | Version after | Unpatched before | after update | after triage |
|---|---|---|---|---|---|
| linux-yocto | 6.18.52+git | 6.18.59+git | 594 | 96 | 90 |
| curl | 8.19.0 | 8.19.0 | 29 | 28 | 20 |
| openssl | 3.5.8 | 3.5.9 | 19 | 5 | 5 |
| glibc | 2.43+git | 2.43+git | 14 | 2 | 2 |
| expat | 2.8.3 | 2.8.3 | 12 | 5 | 5 |
| u-boot | 2026.01 | 2026.01 | 8 | 8 | 0 |
| libpcre2 | 10.47 | 10.47+git | 8 | 0 | 0 |
| busybox | 1.37.0 | 1.37.0 | 4 | 4 | 4 |
| libx11 | 1.8.13 | removed | 4 | 0 | 0 |
| util-linux | 2.41.5 | 2.41.6 | 2 | 1 | 1 |
| acl | 2.3.2 | 2.3.2 | 2 | 2 | 2 |
| tar | 1.35 | 1.35 | 2 | 2 | 2 |
| zlib | 1.3.2 | 1.3.2 | 2 | 2 | 0 |
| util-linux-libuuid | 2.41.5 | 2.41.6 | 2 | 1 | 1 |
| attr | 2.5.2 | 2.5.2 | 1 | 1 | 1 |


## What each status means

The report has three statuses; each entry also has a reason (the `detail`
field). oe-core's mapping is in `meta/conf/cve-check-map.conf`.

| Status | Reason | Set by | Meaning |
|---|---|---|---|
| Patched | `fixed-version` | scanner | the version is at or after the fix in the database's range |
| Patched | `version-not-in-range` | scanner | the version is outside every affected range |
| Patched | `cpe-stable-backport` | recipe (`CVE_STATUS`) | the fix is in the stable-branch commit the recipe builds (oe-core uses it for glibc and the kernel) |
| Patched | `fix-file-included` | build | a patch in `SRC_URI` has a `CVE:` line naming it |
| Ignored | `not-applicable-config` | recipe or scanner | the vulnerable code is not built with this configuration; the scanner sets it for kernel CVEs whose source files were not compiled |
| Ignored | `not-applicable-platform` | recipe | the code is for another architecture, OS or vendor kernel |
| Ignored | `disputed`, `upstream-wontfix`, `cpe-incorrect`, `ignored` | recipe | the CVE itself is contested or does not belong to this software |
| Unpatched | `version-in-range` | scanner | the version is inside an affected range and nothing says otherwise |
| Unpatched | `no-version-ranges` | scanner | the database has no usable range: someone has to look |

## 1. Updates (703 → 157)

No manual decisions; each removed CVE was fixed by a newer upstream version
or backport, or left with the package that carried it.

| Change | Effect on unpatched (installed) |
|---|---|
| oe-core, meta-yocto, bitbake: yocto-6.0.4 tags → wrynose/2.18 heads of 2026-11-11 (+83/+1/+1 commits) | openssl 3.5.8 → 3.5.9 (19 → 5), glibc stable updates (14 → 2), expat patches (12 → 5), libpcre2 branch update (8 → 0), util-linux 2.41.6 |
| linux-yocto 6.18.52 → 6.18.59 (oe-core head has .54; .59 is the linux-yocto branch head, pinned in `linux-yocto_%.bbappend`) | 594 → 96 |
| `DISTRO_FEATURES:remove = "x11 wayland opengl vulkan pulseaudio gobject-introspection-data alsa bluetooth wifi nfc 3g"` | libx11, libxcb, libxau, libxdmcp no longer in the image (4 → 0) |

### Kernel: why each CVE was still open (kernel.org CNA data)

`scripts/kernel-cve-triage.py` groups the unpatched kernel CVEs by the
fix version the kernel.org CNA lists for the 6.18 branch.

Before:

| Kernel 6.18.52+git: why still unpatched | CVEs |
|---|---|
| fixed in 6.18.53 | 145 |
| fixed in 6.18.54 | 55 |
| fixed in 6.18.55 | 69 |
| fixed in 6.18.56 | 45 |
| fixed in 6.18.57 | 143 |
| fixed in 6.18.58 | 1 |
| fixed in 6.18.59 | 42 |
| fixed in 6.18.60 | 19 |
| fixed in 6.18.61 | 14 |
| fixed in 6.18.63 | 1 |
| no 6.18.y fix listed | 14 |
| no 6.18.y fix listed (fixed in mainline only) | 46 |
| **total** | **594** |

After:

| Kernel 6.18.59+git: why still unpatched | CVEs |
|---|---|
| fixed in 6.18.60 | 19 |
| fixed in 6.18.61 | 14 |
| fixed in 6.18.63 | 1 |
| no 6.18.y fix listed | 8 |
| no 6.18.y fix listed (fixed in mainline only) | 48 |
| **total** | **90** |

Method: oe-core wrynose no longer ships a generated kernel exclusion list
(`b737508e63 linux-yocto: remove CVE exclusion list, sbom-cve-check does
this itself`). sbom-cve-check 1.3.5 evaluates the kernel.org CNA's
per-branch ranges and, because the build sets
`SPDX_INCLUDE_COMPILED_SOURCES:pn-linux-yocto = "1"`, marks a CVE
`not-applicable-config` ("Source code not compiled by config") when none of
the files in the CVE's `programFiles` were compiled into this kernel. Of
the 90 left: 34 are fixed in 6.18.60–6.18.63, which linux-yocto had not
merged on 2026-11-11 (wait for the next linux-yocto bump); 48 are fixed
only in mainline so far (watch for stable backports); 8 have no 6.18 fix
data, 7 of them old NVD entries without any version range (below).

## 2. Triage decisions (157 → 133)

Each decision is a `CVE_STATUS` line in a bbappend with the evidence in the
text, or a patch, or a configuration change. All were checked against the
actual build (config files, compiled objects, `libcurl.pc`), not assumed.

### U-Boot 2026.01 (8 → 0), `layers/meta-edge/recipes-bsp/u-boot/u-boot_%.bbappend`

Evidence: `bitbake -c configure u-boot`, then `build/.config`, and the
`Makefile`/`#ifdef` that decides whether the vulnerable function is built.

| CVE | Decision | Evidence |
|---|---|---|
| [CVE-2026-71971](https://nvd.nist.gov/vuln/detail/CVE-2026-71971) (8.8) IP defragmentation | not-applicable-config | `__net_defragment()` is under `#ifdef CONFIG_IP_DEFRAG` in `net/net.c`; `# CONFIG_IP_DEFRAG is not set` |
| [CVE-2026-74220](https://nvd.nist.gov/vuln/detail/CVE-2026-74220), [CVE-2026-74221](https://nvd.nist.gov/vuln/detail/CVE-2026-74221) (8.8) NFS replies | not-applicable-config | `obj-$(CONFIG_CMD_NFS) += nfs.o`; `# CONFIG_CMD_NFS is not set` |
| [CVE-2026-74222](https://nvd.nist.gov/vuln/detail/CVE-2026-74222) (8.8) lwIP wget | not-applicable-config | `obj-$(CONFIG_NET_LWIP) += lwip/`; `# CONFIG_NET_LWIP is not set` (legacy stack) |
| [CVE-2026-74225](https://nvd.nist.gov/vuln/detail/CVE-2026-74225) (7.1) DHCPv6 | not-applicable-config | `obj-$(CONFIG_CMD_DHCP6) += dhcpv6.o`; needs `CONFIG_IPV6`, not set |
| [CVE-2026-71973](https://nvd.nist.gov/vuln/detail/CVE-2026-71973) (5.2) SquashFS | not-applicable-config | `obj-$(CONFIG_FS_SQUASHFS) += squashfs/`; not set |
| [CVE-2026-71974](https://nvd.nist.gov/vuln/detail/CVE-2026-71974) (4.3) `read_slotted_partition()` | not-applicable-config | in `boot/bootmeth_android.c`, built only with `CONFIG_BOOTMETH_ANDROID`; not set |
| [CVE-2026-71972](https://nvd.nist.gov/vuln/detail/CVE-2026-71972) (6.0) RLE8 BMP | **fixed by configuration**, then not-applicable-config | the code *was* built and reachable (`CONFIG_VIDEO_BMP_RLE8=y`; `bmp_display()` is called for PXE menu backgrounds, `CONFIG_CMD_PXE=y`). `files/no-bmp-rle8.cfg` turns off `CONFIG_VIDEO`: the device has no display. Checked: `build/.config` has `# CONFIG_VIDEO is not set`, no `drivers/video/*.o`, no `video_bmp`/`rle8` symbols in the `u-boot` ELF; smoke and A/B tests pass (boot via U-Boot, both slots) |

Two false starts on the RLE8 fix, both caught by checking the result:
`# CONFIG_VIDEO_BMP_RLE8 is not set` alone had no effect because
`CONFIG_VIDEO_LOGO` (default y) `select`s it, and merge_config's log only
shows what the fragment *asked for*; disabling `VIDEO_LOGO` as well did not
link (`undefined reference to __splash_u_boot_logo_begin` from
`video_get_u_boot_logo()`).

### curl 8.19.0 (28 → 20), `layers/meta-edge/recipes-support/curl/curl_%.bbappend`

Sources: curl's own advisories, [vuln.json](https://curl.se/docs/vuln.json)
(machine-readable ranges) and the per-CVE pages. All 28 do affect 8.19.0
by curl's ranges (fixed in curl 8.20.1/8.20.2/8.23.0), so none is a database
error. This build of libcurl (`libcurl.pc` in the sysroot): SSL backend
OpenSSL only; `supported_protocols="FILE FTP FTPS HTTP HTTPS IPFS IPNS"`;
`supported_features="alt-svc AsynchDNS HSTS HTTP2 HTTPS-proxy IDN IPv6
Largefile libz NTLM SSL threadsafe TLS-SRP UnixSockets"`.

| CVE | curl.se says it needs | Decision |
|---|---|---|
| [CVE-2026-87866](https://curl.se/docs/CVE-2026-87866.html), [CVE-2026-82208](https://curl.se/docs/CVE-2026-82208.html) | the wolfSSL backend | not-applicable-config (OpenSSL) |
| [CVE-2026-107928](https://curl.se/docs/CVE-2026-107928.html) | WebSockets with GnuTLS | not-applicable-config (OpenSSL, no WS/WSS) |
| [CVE-2026-87864](https://curl.se/docs/CVE-2026-87864.html) | IMAP | not-applicable-config (no IMAP) |
| [CVE-2026-92389](https://curl.se/docs/CVE-2026-92389.html) | TFTP | not-applicable-config (no TFTP) |
| [CVE-2026-86410](https://curl.se/docs/CVE-2026-86410.html), [CVE-2026-87865](https://curl.se/docs/CVE-2026-87865.html) | SFTP/SCP, libssh2 | not-applicable-config (no libssh2) |
| [CVE-2026-19931](https://curl.se/docs/CVE-2026-19931.html) | Negotiate (SPNEGO) authentication | not-applicable-config (no GSS-API; no SPNEGO feature) |
| [CVE-2026-92392](https://curl.se/docs/CVE-2026-92392.html) HTTP/2 upload use-after-free (curl: **High**) | HTTP/2 | **affected, not fixed here.** The fix (curl commit [8b0744af5b](https://github.com/curl/curl/commit/8b0744af5b1b631da5b938ca31c3f9895ad5a321)) does not apply to 8.19.0, whose `multi_done()` is structured differently, and curl has no 8.20.x maintenance tag to take a backport from. Rewriting connection-lifetime code for a use-after-free without upstream review was not done. Remedy: curl ≥ 8.20.2 (oe-core master has 8.23.0) |
| the other 19 | HTTP/2, cookies, HSTS, FTP, proxies, netrc, native CA store | affected; untriaged (list below). Fixed in curl ≥ 8.20.2 |

### zlib 1.3.2 (2 → 0), `layers/meta-edge/recipes-core/zlib/`

[CVE-2026-85091](https://nvd.nist.gov/vuln/detail/CVE-2026-85091) and
[CVE-2026-76844](https://nvd.nist.gov/vuln/detail/CVE-2026-76844) (8.3):
heap buffer overflow in `gz_write()` after a failed `write()`. Upstream fix
[madler/zlib@df84af25](https://github.com/madler/zlib/commit/df84af25dc1942490e1d1c899a07619152a46148)
(4 lines, "This addresses CVE-2026-85091"; CVE-2026-76844's record cites
the same commit and describes the same missing reset of `strm.next_in`)
applies cleanly to 1.3.2. Carried as a patch with `CVE:` and
`Upstream-Status: Backport [...]` headers, **target only**
(`SRC_URI:append:class-target`): zlib-native is a build tool, and patching it
rebuilt the entire native toolchain (journal section 27).

### Kernel, old NVD entries without version data (13 → 7), `linux-yocto_%.bbappend`

Evidence: `work-shared/qemuarm64/kernel-build-artifacts/.config` (Linux/arm64
6.18.59) and the kernel source.

| CVE | Decision | Evidence |
|---|---|---|
| [CVE-2020-10774](https://nvd.nist.gov/vuln/detail/CVE-2020-10774) `/proc/sys/kernel/rh_features` | not-applicable-platform | Red Hat kernel only; `rh_features` does not occur in the linux-yocto source |
| [CVE-2023-3640](https://nvd.nist.gov/vuln/detail/CVE-2023-3640) `cpu_entry_area` | not-applicable-platform | `cpu_entry_area` exists only under `arch/x86`; this kernel is arm64 |
| [CVE-2022-0400](https://nvd.nist.gov/vuln/detail/CVE-2022-0400) SMC | not-applicable-config | `CONFIG_SMC` not enabled |
| [CVE-2022-1247](https://nvd.nist.gov/vuln/detail/CVE-2022-1247) ROSE | not-applicable-config | `CONFIG_ROSE` not enabled (`CONFIG_HAMRADIO` not set) |
| [CVE-2023-3397](https://nvd.nist.gov/vuln/detail/CVE-2023-3397) JFS | not-applicable-config | `# CONFIG_JFS_FS is not set` |
| [CVE-2023-6238](https://nvd.nist.gov/vuln/detail/CVE-2023-6238) NVMe | not-applicable-config | `# CONFIG_BLK_DEV_NVME is not set`, `# CONFIG_NVME_TARGET is not set` |

Left open (no checkable reason): CVE-1999-0524, CVE-2010-4563,
CVE-2019-14899, CVE-2021-3714, CVE-2021-3864, CVE-2022-4543, CVE-2023-6240. CVE-2022-4543 ("EntryBleed") is described for
x86 KPTI; whether arm64 KPTI is affected was not established, so it stays.

### A decision that was withdrawn: util-linux-libuuid

`util-linux-libuuid` builds only libuuid (`--disable-all-programs
--enable-libuuid`), so
[CVE-2026-53613](https://nvd.nist.gov/vuln/detail/CVE-2026-53613) (a flaw in
`mount`) cannot be in it. A `CVE_STATUS` for that recipe was added and then
removed: both `util-linux` and `util-linux-libuuid` have the same CPE
(`util-linux:2.41.6`), and sbom-cve-check applied the not-applicable
statement to **util-linux as well**, which does build `mount`. A status
that cannot be scoped to the right component would hide a real issue, so
both stay "Unpatched" here. (The upstream fix is a ten-commit series in
util-linux 2.41.5/2.42.4 for the new mount API plus the legacy path; there
is no 2.41.7.)

## 3. Untriaged (133)

90 kernel CVEs (above) and these 43 in user space. They are real entries
for code that is built; nobody has decided anything about them.

| Recipe | CVE | CVSS | Summary |
|---|---|---|---|
| acl 2.3.2 | [CVE-2026-54369](https://nvd.nist.gov/vuln/detail/CVE-2026-54369) | 8.4 (v4) | acl before version 2.4.0 contains a symlink traversal vulnerability in the libacl pathname-based functions acl_get_file(), acl_set_file(), acl_extende… |
| acl 2.3.2 | [CVE-2026-54370](https://nvd.nist.gov/vuln/detail/CVE-2026-54370) | 7.2 (v4) | acl before version 2.4.0 contains a time-of-check to time-of-use (TOCTOU) race condition vulnerability that allows local attackers to escalate privile… |
| attr 2.5.2 | [CVE-2026-54371](https://nvd.nist.gov/vuln/detail/CVE-2026-54371) | 8.4 (v4) | attr before version 2.6.0 contains a symlink traversal vulnerability in the getfattr and setfattr utilities that allows local attackers to escalate pr… |
| busybox 1.37.0 | [CVE-2026-38752](https://nvd.nist.gov/vuln/detail/CVE-2026-38752) | 7.5 (v3) | A stack overflow in the evaluate() function (editors/awk.c) of BusyBox commit 371fe9 allows attackers to cause a Denial of Service (DoS) via supplying… |
| busybox 1.37.0 | [CVE-2026-38753](https://nvd.nist.gov/vuln/detail/CVE-2026-38753) | 7.5 (v3) | A use-after-free in the awk_sub() function (editors/awk.c) of Busybox v1.38.0 allows attackers to cause a Denial of Service (DoS) via supplying a craf… |
| busybox 1.37.0 | [CVE-2026-38755](https://nvd.nist.gov/vuln/detail/CVE-2026-38755) | 7.5 (v3) | A heap overflow in the evalcommand() function (shell/ash.c) of Busybox v1.38.0 allows attackers to cause a Denial of Service (DoS) via supplying a cra… |
| busybox 1.37.0 | [CVE-2026-92205](https://nvd.nist.gov/vuln/detail/CVE-2026-92205) | 6.1 (v3) | BusyBox libarchive Symlink Directory Traversal Arbitrary File Creation Vulnerability. This vulnerability allows remote attackers to create arbitrary f… |
| curl 8.19.0 | [CVE-2026-10536](https://nvd.nist.gov/vuln/detail/CVE-2026-10536) | 9.8 (v3) | A use-after-free vulnerability exists in libcurl when an application configures an HTTP/2 stream-dependency tree via `CURLOPT_STREAM_DEPENDS` or `CURL… |
| curl 8.19.0 | [CVE-2026-107929](https://nvd.nist.gov/vuln/detail/CVE-2026-107929) | 8.1 (v3) | When `CURLOPT_INTERFACE` (or the command line tool's `--interface` option) is set with one of the documented prefixes (`if!`, `host!`, `ifhost!`), tha… |
| curl 8.19.0 | [CVE-2026-11564](https://nvd.nist.gov/vuln/detail/CVE-2026-11564) | 9.1 (v3) | libcurl keeps previously used connections in a connection pool for subsequent transfers to reuse if one of them matches the setup.  An easy handle tha… |
| curl 8.19.0 | [CVE-2026-11856](https://nvd.nist.gov/vuln/detail/CVE-2026-11856) | 9.8 (v3) | Successfully using libcurl to do a transfer to a specific HTTP origin (`hostA`) with **Digest** authentication and then changing the origin to a diffe… |
| curl 8.19.0 | [CVE-2026-18924](https://nvd.nist.gov/vuln/detail/CVE-2026-18924) | 9.1 (v3) | A flaw in libcurl's handling of HTTP/2 Server Push streams, when the parent handle is set to share connections with other handles, can lead to use-aft… |
| curl 8.19.0 | [CVE-2026-80231](https://nvd.nist.gov/vuln/detail/CVE-2026-80231) | 7.5 (v3) | A flaw in libcurl makes it wrongly reuse an existing HTTPS connection setup for a given hostname even when using a different Native CA Store setting (… |
| curl 8.19.0 | [CVE-2026-80255](https://nvd.nist.gov/vuln/detail/CVE-2026-80255) | 7.5 (v3) | A `Set-Cookie:` header using tab (horizontal tab, ASCII code 9) instead of space (ascii code 32) immediately before the `Secure` attribute causes curl… |
| curl 8.19.0 | [CVE-2026-86409](https://nvd.nist.gov/vuln/detail/CVE-2026-86409) | 8.1 (v3) | Setting `CURLOPT_AWS_SIGV4` to NULL but does not disable SigV4 authentication, contrary to expectations and documentation. A subsequent transfer on th… |
| curl 8.19.0 | [CVE-2026-86412](https://nvd.nist.gov/vuln/detail/CVE-2026-86412) | 8.1 (v3) | A remote HTTP server can set a cookie that curl accepts and writes to a Netscape cookie jar but cannot reload. On the next invocation using the same p… |
| curl 8.19.0 | [CVE-2026-86414](https://nvd.nist.gov/vuln/detail/CVE-2026-86414) | 9.1 (v3) | An HTTP proxy can trick libcurl into sending the origin's credentials (Basic password, or a Negotiate/Kerberos ticket) to the proxy itself, by answeri… |
| curl 8.19.0 | [CVE-2026-87862](https://nvd.nist.gov/vuln/detail/CVE-2026-87862) | 8.1 (v3) | Importing a `includeSubDomains` parent HSTS entry before a longer-lived child entry, silently drops the child. Consequently, when the parent expires, … |
| curl 8.19.0 | [CVE-2026-87863](https://nvd.nist.gov/vuln/detail/CVE-2026-87863) | 8.1 (v3) | An authenticated HTTPS origin can supply an HSTS `max-age` value that the parser accepts, but time serialization later fails to process. On 64-bit sys… |
| curl 8.19.0 | [CVE-2026-8926](https://nvd.nist.gov/vuln/detail/CVE-2026-8926) | 9.1 (v3) | When asking curl to use a `.netrc` file to find credentials and at the same time specifying a URL with a username (without a password), like `https://… |
| curl 8.19.0 | [CVE-2026-92386](https://nvd.nist.gov/vuln/detail/CVE-2026-92386) | 8.1 (v3) | When writing a long cookie name and value to a Netscape cookie jar, a remote HTTP server can make `curl` write a record that exceeds its line-length p… |
| curl 8.19.0 | [CVE-2026-92387](https://nvd.nist.gov/vuln/detail/CVE-2026-92387) | 8.1 (v3) | By permanently omitting the final control-response line, a malicious FTP, IMAP, POP3, or SMTP server can exploit libcurl and completely bypass its con… |
| curl 8.19.0 | [CVE-2026-92388](https://nvd.nist.gov/vuln/detail/CVE-2026-92388) | 9.1 (v3) | When an FTP wildcard `LIST` operation fails, libcurl omits the restoration path meant for successful listings, leaving the write callback set to the i… |
| curl 8.19.0 | [CVE-2026-92390](https://nvd.nist.gov/vuln/detail/CVE-2026-92390) | 7.5 (v3) | When invoked with `--remote-header-name` (`-J`), curl fails to preserve its documented anti-clobbering protection if an intermediate 103 Early Hints r… |
| curl 8.19.0 | [CVE-2026-92391](https://nvd.nist.gov/vuln/detail/CVE-2026-92391) | 7.5 (v3) | A deeply nested multipart MIME structure constructed via the public API causes a stack overflow in libcurl during normal cleanup operations. An excess… |
| curl 8.19.0 | [CVE-2026-92392](https://nvd.nist.gov/vuln/detail/CVE-2026-92392) | 8.1 (v3) | During an upload, curl mishandles HTTP/2 `RST_STREAM` received from the server (with reason `HTTP_1_1_REQUIRED` or `REFUSED_STREAM`) leading to a use … |
| curl 8.19.0 | [CVE-2026-94266](https://nvd.nist.gov/vuln/detail/CVE-2026-94266) | 7.5 (v3) | libcurl could accidentally return a URL for `curl_easy_getinfo()`'s `CURLINFO_EFFECTIVE_URL` option containing credentials, even though neither the UR… |
| expat 2.8.3 | [CVE-2026-113351](https://nvd.nist.gov/vuln/detail/CVE-2026-113351) | 8.2 (v4) | libexpat 2.7.2 through 2.9.0 contains a denial of service vulnerability because XML_ParserReset zeroes the direct byte count but never offsets tracked… |
| expat 2.8.3 | [CVE-2026-115506](https://nvd.nist.gov/vuln/detail/CVE-2026-115506) | 8.7 (v4) | libexpat through 2.9.0 contains a denial of service vulnerability because billion laughs protection in accountingDiffTolerated() in xmlparse.c detects… |
| expat 2.8.3 | [CVE-2026-117610](https://nvd.nist.gov/vuln/detail/CVE-2026-117610) | 8.2 (v4) | libexpat through 2.9.0 contains a NULL pointer dereference vulnerability in externalEntityInitProcessor2() in lib/xmlparse.c that mishandles the XML_T… |
| expat 2.8.3 | [CVE-2026-118370](https://nvd.nist.gov/vuln/detail/CVE-2026-118370) | 8.2 (v4) | libexpat through 2.9.0 contains a signed integer overflow vulnerability in storeAtts() in lib/xmlparse.c that allows attackers to trigger out-of-bound… |
| expat 2.8.3 | [CVE-2026-124317](https://nvd.nist.gov/vuln/detail/CVE-2026-124317) | 6.3 (v4) | libexpat through 2.10.0 contains an out-of-bounds read vulnerability in unknown_toUtf8() and unknown_toUtf16() in xmltok.c when handling application-r… |
| glibc 2.43+git | [CVE-2010-4756](https://nvd.nist.gov/vuln/detail/CVE-2010-4756) | 4.0 (v2) | The glob implementation in the GNU C Library (aka glibc or libc6) allows remote authenticated users to cause a denial of service (CPU and memory consu… |
| glibc 2.43+git | [CVE-2026-84243](https://nvd.nist.gov/vuln/detail/CVE-2026-84243) | 7.0 (v3) | The gettext family of functions in the GNU C Library version 2.0 and later accept LANGUAGE environment variable entries that contain '/' or that are e… |
| openssl 3.5.9 | [CVE-2026-107496](https://nvd.nist.gov/vuln/detail/CVE-2026-107496) | — | Issue summary: Encrypting a CMS message to an EC certificate whose group order is 1 may cause CMS_encrypt() or CMS_encrypt_ex() to loop indefinitely. … |
| openssl 3.5.9 | [CVE-2026-88822](https://nvd.nist.gov/vuln/detail/CVE-2026-88822) | — | Issue summary: QUIC, when performing automated peer detection, may allow uninitialized stack data to leak into the received address which is then used… |
| openssl 3.5.9 | [CVE-2026-88823](https://nvd.nist.gov/vuln/detail/CVE-2026-88823) | — | Issue summary: The AES-GCM-SIV implementation treats a zero-length Additional Authenticated Data (AAD) update as a reset of all the AAD supplied so fa… |
| openssl 3.5.9 | [CVE-2026-95691](https://nvd.nist.gov/vuln/detail/CVE-2026-95691) | — | Issue summary: A retransmitted QUIC CRYPTO frame can release packet storage while the TLS record layer retains a direct pointer into that storage caus… |
| openssl 3.5.9 | [CVE-2026-95692](https://nvd.nist.gov/vuln/detail/CVE-2026-95692) | — | Issue summary: Setting an AES-GCM-SIV IV shorter than the expected nonce length via EVP_CipherInit_ex() may trigger a heap out-of-bounds read. Supplyi… |
| tar 1.35 | [CVE-2026-18477](https://nvd.nist.gov/vuln/detail/CVE-2026-18477) | 4.4 (v3) | A TOCTOU (Time-of-Check Time-of-Use) vulnerability in GNU tar's incremental dumpdir 'X' rename handling allows a local attacker with write access to a… |
| tar 1.35 | [CVE-2026-18508](https://nvd.nist.gov/vuln/detail/CVE-2026-18508) | 4.4 (v3) | A flaw was found in GNU tar. When extracting an archive with the --one-top-level option, hardlink targets are not confined to the designated top-level… |
| util-linux 2.41.6 | [CVE-2026-53613](https://nvd.nist.gov/vuln/detail/CVE-2026-53613) | 7.0 (v3) | util-linux is a random collection of Linux utilities. In util-linux 2.17 through 2.41.6 and 2.42 through 2.42.3, and in 2.42.4 when the legacy `mount(… |
| util-linux-libuuid 2.41.6 | [CVE-2026-53613](https://nvd.nist.gov/vuln/detail/CVE-2026-53613) | 7.0 (v3) | util-linux is a random collection of Linux utilities. In util-linux 2.17 through 2.41.6 and 2.42 through 2.42.3, and in 2.42.4 when the legacy `mount(… |

Remedies, roughly by value: curl ≥ 8.20.2 (19 of these and CVE-2026-92392);
openssl 3.5.10 when it is released (5, QUIC/CMS/AES-GCM-SIV); expat 2.10.0
(5); busybox's awk and ash fixes (4; check `CONFIG_AWK` in the image's
busybox config first); acl 2.4.0 / attr 2.6.0 (3); the next linux-yocto
bump (34 kernel CVEs fixed in 6.18.60–63).
