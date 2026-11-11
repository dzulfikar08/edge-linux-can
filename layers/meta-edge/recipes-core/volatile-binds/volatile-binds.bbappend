# With a read-only rootfs, volatile-binds makes /var/lib, /var/cache, /var/spool
# and /srv writable at boot. By default it uses tmpfs (/var/volatile), so the
# contents are lost at every reboot. Here /var/lib goes to the data partition
# instead: an overlay with the rootfs's /var/lib as the read-only lower layer
# and /data/var-lib as the writable upper layer. It holds systemd's state
# (random seed, timers, DynamicUser state directories such as can-demo's).
VOLATILE_BINDS = "\
    /data/var-lib ${localstatedir}/lib\n\
    ${localstatedir}/volatile/cache ${localstatedir}/cache\n\
    ${localstatedir}/volatile/spool ${localstatedir}/spool\n\
    ${localstatedir}/volatile/srv /srv\n\
"
