# Sending the QEMU patch (v1): ready, NOT sent

(Public copy: the author's mail address is replaced by
`<your-real-address>` here and in the patch file. QEMU needs a real
address in `From:` and `Signed-off-by:`; put yours back before sending.)

Everything below was prepared and checked on 2026-11-12. Nothing has been
sent to any list. Sending needs your SMTP credentials, so it is yours to do.

## The patch

[0001-hw-net-can-ctucan-report-fault-confinement-state-rai.patch](0001-hw-net-can-ctucan-report-fault-confinement-state-rai.patch)

- Subject: `[PATCH] hw/net/can/ctucan: report fault confinement state, raise FCSI on enable`
- Author and DCO: `Dzulfikar Ats Tsauri <your-real-address>`
  (`Signed-off-by` with the real address, as QEMU requires; the commit was
  made with that identity in the QEMU clone only)
- Base: QEMU master `29760efaf96bbe5928f8111881eddbf5ac06439e`
  (2026-11-10, "Update version for v11.2.0-rc0 release"); `git apply --check`
  clean on a pristine checkout of it
- `scripts/checkpatch.pl`: 0 errors, 0 warnings
- Single patch, so no cover letter. The test report is below the `---`
  line (it goes to the list, not into git history)

## Tested

| Test | Result |
|---|---|
| Build `qemu-system-aarch64` from master + patch (`--target-list=aarch64-softmmu`), gcc 16.2 | 3.6 min, no warnings in `ctucan_core.c` |
| [boot-test.py](boot-test.py): edge-image (Linux 6.18.59 arm64) on `-M virt` with `-device ctucan_pci` bridged to host `vcan0` | **9/9** with the patch: ERROR-ACTIVE, `ISR = 0x00000010`, `STOPPED -> ERROR_ACTIVE`, STOPPED when down and ERROR-ACTIVE again after up, classic + FD frames both ways |
| Same test on master **without** the patch | 6/9: frames pass, state stays STOPPED (the bug) |
| `tests/qtest/xlnx-can-test`, `xlnx-canfd-test` | 5/5, 3/3 (shared CAN bus core; there is no ctucan qtest) |

Evidence: [test-evidence.txt](test-evidence.txt).

## Recipients (`scripts/get_maintainer.pl -f hw/net/can/ctucan_core.c` on master)

```
Pavel Pisa <pisa@cmp.felk.cvut.cz> (maintainer:CAN bus subsystem...)
Francisco Iglesias <francisco.iglesias@amd.com> (maintainer:CAN bus subsystem...)
Vikram Garhwal <vgarhw@gmail.com> (maintainer:CAN bus subsystem...)
Jason Wang <jasowangio@gmail.com> (odd fixer:Network devices, gitlab:@jasowang)
qemu-devel@nongnu.org (open list:All patches CC here)
```

## The command

One-time setup (Gmail needs an app password; any SMTP server works):

```sh
git config --global sendemail.smtpServer smtp.gmail.com
git config --global sendemail.smtpServerPort 587
git config --global sendemail.smtpEncryption tls
git config --global sendemail.smtpUser <your-real-address>
```

Then, from this repository's root (a QEMU checkout is not needed to send a
ready patch file):

```sh
git send-email \
  --from="Dzulfikar Ats Tsauri <your-real-address>" \
  --to=qemu-devel@nongnu.org \
  --cc="Pavel Pisa <pisa@cmp.felk.cvut.cz>" \
  --cc="Francisco Iglesias <francisco.iglesias@amd.com>" \
  --cc="Vikram Garhwal <vgarhw@gmail.com>" \
  --cc="Jason Wang <jasowangio@gmail.com>" \
  docs/upstream/0001-hw-net-can-ctucan-report-fault-confinement-state-rai.patch
```

Try it first with `--dry-run` added (prints the mail, sends nothing), or
send it to yourself only (`--to=<your-real-address>`, no `--cc`) to see
how it arrives.

## Before sending, re-check

1. Master has moved: rebase and re-run checkpatch (on a QEMU clone with the
   commit: `git pull --rebase`, `scripts/checkpatch.pl HEAD^..HEAD`,
   `git format-patch -1`), and `scripts/get_maintainer.pl` again.
2. The patch says "I can write [a qtest] if that is wanted": only keep that
   offer if you are willing to do it.

## After sending

- The mail appears on https://lore.kernel.org/qemu-devel/ and
  https://patchew.org/QEMU/ within minutes; patchew runs its own checks.
- Reply to review comments on the list (plain text, inline quoting, no top
  posting). Changes go into a `v2` (`git format-patch -v2`), with a short
  changelog below the `---` line, sent as a new thread.
- A maintainer adds `Reviewed-by:` and takes the patch into their tree;
  then it reaches master in a pull request.
- Then change the Yocto copy's header
  (`layers/meta-edge/recipes-devtools/qemu/files/0001-hw-net-can-ctucan-report-fault-confinement-state.patch`)
  from `Upstream-Status: Pending` to `Submitted [<lore link>]`, later
  `Backport [<commit URL>]`, and drop the bbappend once Yocto's QEMU has it.
