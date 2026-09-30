# cups-browsed without network-online.target (optional, distro-specific)

Debian's `cups-browsed.service` is `After=`/`Wants=network-online.target`, which pulls
`NetworkManager-wait-online` into the boot and holds the desktop about 5 s. cups-browsed finds
printers when the network comes up anyway, so on this tablet the unit can be overridden with a
copy that drops `network-online.target`:

```sh
sed 's/ network-online.target//g' /usr/lib/systemd/system/cups-browsed.service \
    > /etc/systemd/system/cups-browsed.service
systemctl daemon-reload
```

(Drop-ins cannot remove entries from `After=`/`Wants=`, so a full copy is needed. Re-run after a
cups-browsed package update changes the unit.)
