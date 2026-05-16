# workplacesim Pi deploy

Cross-build, copy, and run workplacesim on a Raspberry Pi 1 (armv6, hard-float)
running Alpine Linux + OpenRC. The service owns `tty1` and draws to
`/dev/fb0`; mDNS-SD is advertised by the binary itself (no `avahi-daemon`
running in userspace).

## What the deploy produces

- A single ~5 MB static-ish Rust binary at `/usr/local/bin/workplacesim`.
- An OpenRC service (`/etc/init.d/workplacesim`) enabled in the `default`
  runlevel, supervised by `supervise-daemon` for `Restart=always`-equivalent
  respawn on crash.
- Pi hostname set to `workplacesim` (or whatever `--hostname` you passed),
  so clients on the LAN reach it at `workplacesim.local:4317` over mDNS.
- `tty1` getty disabled so the renderer owns the VT.

Targets we expect to verify on hardware: ~50 MB Alpine install footprint and
~10 s cold boot to first frame. Treat those as goals, not measurements.

## Prerequisites

On the **Mac**:

- [`cross`](https://github.com/cross-rs/cross) — `cargo install cross`.
- Docker Desktop (or equivalent) running. `cross` uses it for the
  `arm-unknown-linux-gnueabihf` toolchain image.
- `ssh`, `scp` on `$PATH`.

On the **Pi**:

- A Raspberry Pi 1 (or Zero W) with Alpine Linux flashed and booted. The
  stock `alpine-rpi` tarball is fine — it already ships OpenRC and
  `busybox-openrc`.
- `sshd` running, your key in `~/.ssh/authorized_keys` for either `root` or
  a sudo/doas-capable user.
- HDMI attached, LAN connected.

### Pi boot config

Edit `/boot/config.txt` (or `/media/mmcblk0p1/config.txt` from the Alpine
side, since `/boot` isn't usually auto-mounted):

- `framebuffer_depth=32` or `framebuffer_depth=16` — renderer auto-detects
  XRGB8888 (32bpp) and RGB565 (16bpp). Other depths are rejected.
- `hdmi_force_hotplug=1` — keep HDMI output on even without EDID.
- `disable_overscan=1` — no black border around the canvas.
- Optional: `hdmi_group=2`, `hdmi_mode=82` to force 1080p if autodetect
  guesses wrong. The renderer scales aspect-preserving nearest-neighbour to
  any detected resolution with pillar/letterbox.

## Deploy paths

### Iterative: `install.sh`

```sh
cd rust/workplacesim
./deploy/install.sh root@<host>
```

What it does over SSH:

1. `apk add openrc busybox-openrc` if `rc-service` is missing (stock rpi
   tarball already has these; no-op on an existing install).
2. Sets `/etc/hostname` and rewrites the relevant `/etc/hosts` line.
3. Comments out the `tty1::respawn:/sbin/getty …` line in `/etc/inittab` and
   kills the running getty so our service can take the VT on first start.
4. Installs the binary to `/usr/local/bin/workplacesim` and the init script
   to `/etc/init.d/workplacesim`.
5. `rc-update add workplacesim default && rc-service workplacesim restart`.

Re-running the script is safe: it replaces the binary, keeps the rc-update
idempotent, and bounces the service. No avahi, no systemctl, no
hostnamectl.

### Pre-baked image: `mkimage.sh`

`mkimage.sh` produces a 512 MiB `.img` file containing the stock Alpine
Linux rpi image plus a `workplacesim.apkovl.tar.gz` overlay that carries
the musl-static binary, the OpenRC service, runlevel wiring, and
optionally pre-installed `openssh` + your SSH public key + WiFi creds +
the `linux-firmware-rtlwifi` blobs and `wpa_supplicant`. `dd` the image
to an SD card, boot the Pi, and workplacesim is on HDMI with mDNS live,
sshd accepting your key, and (if you baked wifi in) wlan0 auto-joining
— no post-flash SSH setup.

```sh
# Bare bake (eth0=DHCP, no ssh server, no wifi):
./deploy/mkimage.sh

# Full bake (recommended for the canonical Pi-1-on-Mac workflow):
./deploy/mkimage.sh \
    --output ./workplacesim-pi.img \
    --wifi-ssid "MyNetwork" --wifi-psk "supersecret" \
    --ssh-pubkey ~/.ssh/id_ed25519.pub \
    --static-ip 192.168.2.10/24 --gateway 192.168.2.1

./deploy/mkimage.sh --dry-run   # print plan, touch nothing (works on macOS)
```

Flags:

- `--wifi-ssid` + `--wifi-psk` — bake `/etc/wpa_supplicant/wpa_supplicant.conf`
  and the firmware/driver packages so `wlan0` auto-joins on first boot.
  Both or neither.
- `--ssh-pubkey FILE` — install `openssh`, generate sshd host keys
  (baked into the image, so SSH known_hosts doesn't churn across
  reflashes), and drop your public key in `/root/.ssh/authorized_keys`.
  Triggers `sshd` in the default runlevel.
- `--static-ip ADDR/PREFIX` + `--gateway ADDR` — eth0 gets a static
  address instead of DHCP. The canonical Mac+Pi direct-cable workflow
  uses `192.168.2.10/24` with the Mac sharing wifi out as
  `192.168.2.1` (System Settings → Sharing → Internet Sharing).
- `--hostname NAME` — defaults to `workplacesim`; that's also the mDNS
  name (`<NAME>.local`).
- `--alpine-version VER` — pin Alpine release (default `3.20.3`).
- `--output PATH` — image path (default `./workplacesim-pi.img`).

**Linux-only for the real bake.** Loop devices, `parted`, `mkfs.vfat`,
and `apk-tools` aren't available on macOS. The recommended workflow on
macOS is a privileged `alpine:3.20` Docker container:

```sh
docker run --rm --privileged \
  -v "$PWD":/work \
  -v "$HOME/.cache/workplacesim":/root/.cache/workplacesim \
  -v "$HOME/.ssh/id_ed25519.pub":/host_pubkey:ro \
  -w /work/rust/workplacesim \
  alpine:3.20 \
  sh -c '
    apk add --no-cache bash parted dosfstools util-linux e2fsprogs \
                       curl tar gzip coreutils openssh-keygen
    ./deploy/mkimage.sh \
        --output /work/workplacesim-pi.img \
        --wifi-ssid "MyNetwork" --wifi-psk "supersecret" \
        --ssh-pubkey /host_pubkey \
        --static-ip 192.168.2.10/24 --gateway 192.168.2.1
  '
```

`--dry-run` works on macOS — it prints the plan without touching any
filesystem.

### Building the musl binary

The binary the apkovl carries must be statically linked against musl
(Alpine has no glibc). The `mkimage.sh` script looks for a binary at
`target-pi-musl/arm-unknown-linux-musleabihf/release/workplacesim` and
falls back to `target/arm-unknown-linux-gnueabihf/release/workplacesim`
with a warning if only the glibc one is present.

To build the musl target on any host (no separate cross-toolchain
needed — rust-lld does the linking):

```sh
docker run --rm \
  -v "$PWD":/work \
  -w /work/rust/workplacesim \
  -e CARGO_TARGET_DIR=/work/target-pi-musl \
  rust:alpine \
  sh -c '
    apk add --no-cache musl-dev build-base
    rustup target add arm-unknown-linux-musleabihf
    RUSTFLAGS="-C target-cpu=arm1176jzf-s -C linker=rust-lld -C link-self-contained=yes" \
      cargo build --target arm-unknown-linux-musleabihf --release --features fb --no-default-features
  '
```

The produced binary is fully self-contained (musl libc statically
linked in), so it runs on Alpine with no dynamic dependencies at all.

**How the image boots (diskless mode).** The stock `alpine-rpi` tarball
is a "diskless"-mode image. The SD card has a single FAT32 partition
that holds only boot firmware (`bootcode.bin`, `start*.elf`,
`fixup*.dat`), the kernel, `cmdline.txt`, the modloop, and any
`*.apkovl.tar.gz` overlays. There is no conventional Linux rootfs on
disk. At boot:

1. Firmware loads the kernel.
2. The initramfs mounts a **tmpfs as `/`** and populates it from the
   modloop + apk cache.
3. The initramfs scans the FAT root for `*.apkovl.tar.gz` and extracts
   each one on top of the in-memory root.
4. OpenRC runs `sysinit` → `boot` → `default` (per `/etc/inittab`).

All our customizations therefore live in a single
`workplacesim.apkovl.tar.gz` that `mkimage.sh` assembles from
`deploy/image-overlay/` plus `deploy/workplacesim.initd` and the
cross-built binary. **Files written straight to FAT paths like
`/etc/init.d/...` would never be read** — that's why the script does
not overlay anything onto FAT except the apkovl itself.

**Apkovl contents** (paths relative to the tmpfs root after extraction):

- `/usr/local/bin/workplacesim` — the musl-static binary, with mDNS
  registration in-process.
- `/etc/init.d/workplacesim` — the OpenRC service, copied from
  `deploy/workplacesim.initd` at bake time (single source of truth).
- `/etc/hostname` — `--hostname` value (default `workplacesim`).
- `/etc/inittab` — Alpine default with `tty1` getty commented out.
- `/etc/network/interfaces` — generated from `--static-ip` / DHCP and
  optionally a `wlan0` block with `pre-up /sbin/wpa_supplicant ...`.
- `/etc/resolv.conf` — `nameserver 1.1.1.1` as a default; gets
  overwritten by udhcpc when DHCP succeeds.
- `/etc/apk/repositories` — local `/media/mmcblk0p1/{apks,cache}` plus
  the online Alpine repos, so `apk add` works both offline (from the
  baked-in cache) and online.
- `/etc/apk/world` — declares the packages the image is supposed to
  have (`alpine-base` plus optional `openssh`, `wpa_supplicant`,
  `linux-firmware-rtlwifi`).
- `/etc/runlevels/sysinit/{devfs,dmesg,hwdrivers,mdev,modloop,sysfs}`,
  `/etc/runlevels/boot/{bootmisc,hostname,hwclock,modules,swclock,sysctl,syslog}`,
  `/etc/runlevels/default/{local,networking,workplacesim}` and
  optionally `sshd`. The apkovl extraction merges these into the base
  Alpine init system; **all** services the Pi needs at boot must be
  symlinked here because tar's directory-merge effectively wipes the
  empty `/etc/runlevels/*` dirs the base ships with. The load-bearing
  one is `modloop` — without it kernel modules don't load and wlan
  drivers can't bind to the dongle.
- `/etc/local.d/workplacesim.start` — belt-and-suspenders
  `rc-service workplacesim start` + self-delete.

If you passed `--ssh-pubkey`:

- `/sbin/sshd`, `/usr/bin/ssh-keygen`, etc. (the openssh package
  contents, pre-extracted from the .apk so they're available before
  the Pi runs any apk-add at boot).
- `/etc/ssh/ssh_host_{ed25519,rsa,ecdsa}_key{,.pub}` — host keys
  generated at bake time. Same keys across reflashes so SSH
  known_hosts doesn't keep complaining.
- `/etc/ssh/sshd_config` — `PermitRootLogin yes` + pubkey auth on.
- `/root/.ssh/authorized_keys` — the file you passed.

If you passed `--wifi-ssid`/`--wifi-psk`:

- `/sbin/wpa_supplicant`, `/lib/firmware/rtlwifi/*` (etc.) — also
  pre-extracted from the matching armhf .apks.
- `/etc/wpa_supplicant/wpa_supplicant.conf` — your SSID + PSK, mode
  `0600`.

The image's FAT root also gets a `/cache/` directory containing the
`.apk` files for the optional packages, so `apk add` on the running Pi
can find them offline.

**RAM / tmpfs watch-out.** Because `/` is tmpfs, everything in the
apkovl consumes RAM at runtime. Our binary is ~5 MB, which is modest
even on a 512 MB Pi 1 Model B — but keep an eye on total apkovl size
if you start shipping fonts, extra libraries, or other assets. The
modloop + apk cache already claim a chunk of RAM; a bloated apkovl
compounds the pressure.

**WiFi.** Pass `--wifi-ssid` + `--wifi-psk` to bake it in. Heads-up:
not every USB dongle Just Works on Alpine's stock `linux-rpi` kernel,
even with `linux-firmware-rtlwifi`. The Realtek RTL8188CUS we developed
this against (USB 0x0BDA:0x8176) has the right firmware but didn't
auto-bind to `rtl8xxxu` on first boot in our testing — likely a
re-enumeration ordering issue specific to that chip. Ethernet works
unconditionally; if you're using a direct Mac↔Pi cable, also pass
`--static-ip 192.168.2.10/24 --gateway 192.168.2.1`.

**Prerequisites for a real bake:** `curl`, `sha256sum`, `losetup`,
`parted`, `mkfs.vfat`, `partx`, `tar`, `mount`. Plus `apk` (`apk-tools`
package) and `ssh-keygen` if you enable wifi or SSH. The Alpine tarball
is cached under `$XDG_CACHE_HOME/workplacesim/` (default
`~/.cache/workplacesim/`) and SHA256-verified on every run. The script
uses GNU tar flags; standard Linux `tar` is GNU tar.

**Overlay tree.** `deploy/image-overlay/` is the source of truth for
the apkovl contents. `mkimage.sh` copies from there into a staging
directory, adds the initd + binary + runlevel symlinks, then tars the
whole thing with `tar --owner=root --group=root --numeric-owner -C
staging -czf workplacesim.apkovl.tar.gz .` (note the leading `.`, which
produces the `./etc/...` relative paths the apkovl extraction expects).
Add per-site tweaks in `image-overlay/` (custom hostname, different
`/etc/network/interfaces`, extra `/etc/local.d/*.start` hooks) rather
than editing the script.

### Install flags

- `--hostname <name>` — use a hostname other than `workplacesim`. Clients
  then reach the Pi at `<name>.local:4317`.
- `--skip-hostname` — leave the existing Pi hostname alone. mDNS will
  advertise under whatever hostname is already set.
- `--status-only` — skip build + copy; just print service status and tail
  the log files.

### Multiple Pis on one LAN

Give each Pi a distinct hostname (`--hostname wpsim-lab`, `--hostname
wpsim-desk`). Without that, mDNS resolves collisions by suffixing the
second to `workplacesim-2.local`, defeating the predictable-URL promise.

## Why not bare-metal / no-std

The server stack is `tokio` + `axum` + `serde`, all of which are deeply
tied to `std`. HTTP + SSE + mDNS parity with the Node backend is a hard
requirement (the Rust port is a drop-in replacement for the same plugin
hooks). A bare-metal or no-std rewrite would effectively be a rewrite of
all three plus a TCP/IP stack on Embassy or similar — weeks of work for
a marginal footprint win on a non-battery-powered appliance.

Alpine + OpenRC is the sweet spot: Linux where we want Linux (framebuffer,
VT ioctls, networking), BusyBox where we don't need GNU, and OpenRC's
`supervise-daemon` giving us crash-respawn for free.

## Post-deploy

```sh
ssh root@<host> 'tail -f /var/log/workplacesim.log'   # tail logs
ssh root@<host> 'rc-service workplacesim status'     # service state
./deploy/install.sh root@<host> --status-only         # both in one shot
```

## Driving it from the Mac

Claude Code hook POSTs target `http://127.0.0.1:4317` by default. Point them
at the Pi:

```sh
export WORKPLACESIM_URL=http://workplacesim.local:4317
```

Put that in your shell rc so every Claude Code session reaches the Pi.
`install.sh` prints this exact line at the end of a successful deploy.

If `.local` resolution is blocked on your LAN (some enterprise networks
filter mDNS), fall back to the Pi's IP:
`export WORKPLACESIM_URL=http://<ip>:4317`.

## Browser debug

The server embeds `public/index.html` and `public/main.js` and serves them
on `/`. Open `http://workplacesim.local:4317/` on your Mac to watch the
same scene Phaser renders, alongside what the Pi is drawing to HDMI. SSE
lives at `/events`.

To confirm the mDNS advertisement from the Mac:
`dns-sd -B _workplacesim._tcp` should list the running instance.

## Live tuning via `/config`

Open `http://workplacesim.local:4317/config` from a laptop on the same
network. The page shows server status (uptime, active sims, event rate,
detected fb geometry) and live-editable settings for sim motion, effect
density, lifecycle TTLs, and display.

Changes persist to a JSON file resolved from (in order):
`$WORKPLACESIM_CONFIG_PATH`, `$XDG_CONFIG_HOME/workplacesim/config.json`
or `$HOME/.config/workplacesim/config.json`, then
`./workplacesim-config.json` next to the binary.

Fields that need the service to bounce (window size / fullscreen on the fb
build) are tagged `[restart]` in the form; `POST /api/restart` exits the
process, and `supervise-daemon` brings it back within ~2 seconds.

## Uninstall

```sh
ssh root@<host> '
  rc-service workplacesim stop
  rc-update del workplacesim default
  rm -f /etc/init.d/workplacesim /usr/local/bin/workplacesim
  # Re-enable tty1 getty if you want the login prompt back:
  sed -i "s|^#\\([[:space:]]*tty1::respawn.*\\)|\\1|" /etc/inittab
  kill -HUP 1
'
```

## Troubleshooting

**Getting a shell while the renderer holds the VT.** The install disables
the tty1 getty, so plugging in a USB keyboard and hitting `Ctrl+Alt+F2`
won't drop you into a login — all VT gettys are off. The debug path is
SSH:

```sh
ssh root@workplacesim.local
rc-service workplacesim stop   # frees /dev/fb0 and the VT
# ...poke at things...
rc-service workplacesim start
```

If you want a login on tty2 as a backup, uncomment the `tty2::respawn`
line in `/etc/inittab` before deploy, or add one, and `kill -HUP 1`.

**Blank screen.** Framebuffer depth mismatch. `tail /var/log/workplacesim.err`
will name the variable and its size — fix `framebuffer_depth=32` in
`config.txt`, reboot.

**Unexpected resolution.** The renderer logs the framebuffer mode from
`FBIOGET_VSCREENINFO` on startup (e.g.
`fb: 1024x768 Rgb565 stride=2048 fit=1024x576@(0,96)`). Any resolution is
fine — the scaler preserves aspect ratio with pillarbox/letterbox. Only
force `hdmi_group`/`hdmi_mode` if you specifically want a different mode.

**Hook POSTs not landing.** Verify `WORKPLACESIM_URL` on the Mac and that
the Pi's port 4317 is reachable (`curl http://<host>:4317/api/agents`
should return `{"agents":[...]}`). Check LAN firewall rules. If
`.local` resolution fails, try the Pi's IP directly.

**Service flapping.** `supervise-daemon` will keep respawning on crash
with a 2 s delay. `tail -n 200 /var/log/workplacesim.err` for the panic
trail.

**mDNS doesn't advertise immediately.** First-run registration can take a
few seconds; the responder in the binary waits for the network to be up.
If it never shows, check `rc-service networking status` and that
multicast isn't being blocked on the LAN.
