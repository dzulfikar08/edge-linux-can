# Upstream: QEMU CTU CAN FD fault confinement patch

Status: **ready to send (v1), not sent.** Sending needs the author's SMTP
credentials; how and to whom: [SEND.md](SEND.md).

| File | What |
|---|---|
| [0001-hw-net-can-ctucan-report-fault-confinement-state-rai.patch](0001-hw-net-can-ctucan-report-fault-confinement-state-rai.patch) | `git format-patch` v1 against QEMU master `29760efaf9`, real DCO identity, test report below `---` |
| [SEND.md](SEND.md) | recipients (from `get_maintainer.pl`), the exact `git send-email` command, what to check before and do after |
| [boot-test.py](boot-test.py) | boots edge-image with a given `qemu-system-aarch64` and checks the controller state and traffic (run on the host) |
| [test-evidence.txt](test-evidence.txt) | the boot test with and without the patch on master, and the CAN qtests |

The same change for QEMU 10.2.0, which oe-core wrynose builds, is carried in
this repository as
`layers/meta-edge/recipes-devtools/qemu/files/0001-hw-net-can-ctucan-report-fault-confinement-state.patch`
(`Upstream-Status: Pending`). Background: learning guide
[chapters 27–29](../learning-guide.md#27-debugging-a-device-model-qemu-against-the-linux-driver).
