# A/B boot selection with boot counting, in the variable names RAUC's U-Boot
# backend reads and writes (https://rauc.readthedocs.io/en/latest/integration.html#u-boot):
#   BOOT_ORDER    slots in order of preference, e.g. "A B". RAUC puts the slot it
#                 just installed first.
#   BOOT_x_LEFT   boot attempts left for slot x. Every boot of x uses one. A boot
#                 that the system itself judges healthy runs `rauc status
#                 mark-good`, which sets it back to 3. A slot with 0 left is
#                 skipped, so a slot that keeps failing falls back to the other one.
test -n "${BOOT_ORDER}" || setenv BOOT_ORDER "A B"
test -n "${BOOT_A_LEFT}" || setenv BOOT_A_LEFT 3
test -n "${BOOT_B_LEFT}" || setenv BOOT_B_LEFT 3

setenv rauc_slot
setenv root_part
# (U-Boot's shell is a small "hush" shell: keep conditions simple.)
for slot in "${BOOT_ORDER}"; do
  if test -z "${rauc_slot}"; then
    if test "${slot}" = "A"; then
      if test ${BOOT_A_LEFT} -gt 0; then
        setexpr BOOT_A_LEFT ${BOOT_A_LEFT} - 1
        setenv rauc_slot A
        setenv root_part 2
      fi
    fi
    if test "${slot}" = "B"; then
      if test ${BOOT_B_LEFT} -gt 0; then
        setexpr BOOT_B_LEFT ${BOOT_B_LEFT} - 1
        setenv rauc_slot B
        setenv root_part 3
      fi
    fi
  fi
done

if test -z "${rauc_slot}"; then
  # Both slots used up their attempts: give both another 3 and start over.
  echo "edge-boot: no slot has attempts left; resetting counters"
  setenv BOOT_A_LEFT 3
  setenv BOOT_B_LEFT 3
  saveenv
  reset
fi

echo "edge-boot: slot ${rauc_slot} (partition ${root_part}), attempts left A=${BOOT_A_LEFT} B=${BOOT_B_LEFT}, order ${BOOT_ORDER}"
# Store the decremented counter before booting, so a crash or hang during
# this boot still counts as one attempt.
saveenv

# The root filesystem is mounted read-only from the start ("ro"). panic=10:
# a kernel that panics reboots after 10 s, which also uses up an attempt.
setenv bootargs "root=/dev/vda${root_part} rootfstype=ext4 ro rootwait console=ttyAMA0,115200 panic=10 rauc.slot=${rauc_slot}"

# The kernel lives inside the slot (/boot/Image), so it is updated together
# with the rootfs. The device tree is the one QEMU generated for this machine
# (CPUs, memory and the PCI devices given on the command line); U-Boot got it
# from QEMU and keeps it at ${fdtcontroladdr}.
ext4load virtio 0:${root_part} ${kernel_addr_r} /boot/Image
booti ${kernel_addr_r} - ${fdtcontroladdr}
