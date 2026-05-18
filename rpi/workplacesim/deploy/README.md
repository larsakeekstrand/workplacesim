# workplacesim Pi deploy

## What you get

A Raspberry Pi 1 (or any compatible Pi) running workplacesim as a
systemd-managed service. The binary owns `tty1`, draws the scene to
`/dev/fb0` over HDMI, and serves HTTP + SSE on port 4317. The Pi is
reachable at `<hostname>.local:4317` via an in-binary mDNS responder that
announces `_workplacesim._tcp` on the LAN — no `avahi-daemon` required.
SSH is key-only.

## Why Pi OS over Alpine

Older USB wifi dongles like the RTL8188CUS need Pi Foundation kernel
patches that aren't in Alpine's mainline kernel. Pi OS Lite ships a kernel
that handles those dongles out of the box. If you're ethernet-only, Alpine
is lighter — that path is in git history before the revert commit but no
longer maintained.

## Prerequisites

- Mac (or Linux dev host) with Docker running. `cross` uses Docker for the
  arm toolchain image.
- `cross` installed: `cargo install cross`.
- `rpi-imager` installed: https://www.raspberrypi.com/software/.
- Your SSH pubkey at `~/.ssh/id_ed25519.pub` (or whichever key you'll
  authorize on the Pi).
- Pi 1 (or compatible Pi) with power, HDMI cable, and SD card.

## Flash Pi OS Lite via rpi-imager

Launch `rpi-imager` and go through it in order:

1. **Choose Device** — Raspberry Pi 1. This auto-selects the 32-bit armhf
   image, which matches the `arm-unknown-linux-gnueabihf` cross-compile
   target the install script uses.
2. **Choose OS** — Raspberry Pi OS Lite (32-bit), under "Raspberry Pi OS
   (other)".
3. **Choose Storage** — the SD card.
4. Click **Next**; when prompted, click **Edit Settings** to open the OS
   Customisation dialog (the gear-icon dialog).
   - **General** tab:
     - Set hostname (e.g. `workplacesim`). Clients will reach the Pi at
       `<hostname>.local:4317`.
     - Set username + password. Default is `pi`; if you pick something
       else, pass it to `install.sh` via `--user`.
     - Configure wireless LAN: SSID, PSK, wifi country code.
     - Set locale (timezone + keyboard).
   - **Services** tab:
     - Enable SSH.
     - Select **Allow public-key authentication only** and paste the
       contents of `~/.ssh/id_ed25519.pub`.
   - Save the customisation.
5. **Write** the image.
6. Insert the SD card, plug HDMI + power into the Pi. First boot takes
   roughly two minutes — Pi OS expands the filesystem and applies the
   customisation on first boot.
7. Confirm: `ssh pi@workplacesim.local` (or whatever hostname/user you
   set) should connect without a password.

## Pre-deploy Pi config (one-time)

Two things to set up on the Pi once before the first `install.sh` run.

### Passwordless sudo for the deploy user

`install.sh` runs `sudo` commands over a non-interactive SSH session, so
`sudo` can't prompt for a password. Grant the deploy user passwordless
sudo (one line, requires entering the password once):

```sh
ssh -t pi@workplacesim.local 'echo "pi ALL=(ALL) NOPASSWD: ALL" | sudo tee /etc/sudoers.d/010_pi-nopasswd >/dev/null && sudo chmod 0440 /etc/sudoers.d/010_pi-nopasswd && sudo -n true && echo OK'
```

`-t` allocates a pty so `sudo` can prompt. If you used `--user` to pick a
non-`pi` username, substitute it in both the ssh target and the sudoers
line.

### Framebuffer + HDMI config

Edit `/boot/firmware/config.txt` on the Pi:

```sh
ssh pi@workplacesim.local sudo nano /boot/firmware/config.txt
```

Set:

- `framebuffer_depth=32` or `framebuffer_depth=16` — the renderer
  auto-detects XRGB8888 (32bpp) and RGB565 (16bpp). Other depths are
  rejected with a message pointing back at this knob.
- `hdmi_force_hotplug=1` — keep HDMI output on even without EDID
  (headless boot).
- `disable_overscan=1` — no black border around the canvas.

Reboot the Pi (`sudo reboot`). With HDMI auto-detect the framebuffer mode
usually works without forcing `hdmi_group`/`hdmi_mode`; only override them
if autodetect picks the wrong resolution.

## Run `install.sh`

```sh
cd rpi/workplacesim
./deploy/install.sh pi@workplacesim.local
```

The script runs `cross build --target arm-unknown-linux-gnueabihf
--release --features fb --no-default-features`, `scp`s the binary + service
unit to the Pi, and `ssh`es in to enable and restart the service. It also
disables `getty@tty1` (so it doesn't fight the renderer for the console)
and disables `avahi-daemon` if it finds it active (in-binary mDNS would
collide with it).

### Boot-time trimming (idempotent)

The same SSH block applies a set of one-time tweaks that shave several
seconds off cold boot on a single-purpose kiosk Pi:

- Masks `apt-daily.timer`, `apt-daily-upgrade.timer`, `man-db.timer`.
- Disables `triggerhappy`, `keyboard-setup`, `console-setup`,
  `bluetooth`, `hciuart` — input/keyboard/BT machinery the visualizer
  doesn't use.
- Appends `disable_splash=1`, `boot_delay=0`, `dtoverlay=disable-bt` to
  `/boot/firmware/config.txt` (skipping splash, the kernel boot countdown,
  and BT firmware init).
- Adds `quiet loglevel=3` to `/boot/firmware/cmdline.txt` to suppress
  kernel boot chatter on the console workplacesim takes over.

`config.txt` / `cmdline.txt` edits need a reboot to take effect; the
script prints a note when it modifies them. The systemd unit no longer
declares `Wants=network-online.target` — `bind(0.0.0.0:4317)` doesn't
need an IP, and waiting for wpa_supplicant + DHCP was ~20-30s of black
screen on Pi 1 wifi.

### Install flags

- `--user <user>` — SSH login user (default `pi`). Use this if your
  `rpi-imager` customisation chose a different username.
- `--hostname <name>` — set the Pi hostname during deploy. Default is to
  leave whatever `rpi-imager` already set in place.
- `--skip-hostname` — explicit no-op alias for the default.
- `--status-only` — skip build + copy; just print service status and
  recent logs.

### Multiple Pis on one LAN

Give each Pi a distinct hostname at `rpi-imager` flash time (e.g.
`wpsim-lab`, `wpsim-desk`), or pass `--hostname` to `install.sh`. Without
that, mDNS resolves name collisions by suffixing the second to
`workplacesim-2.local`, which defeats the predictable-URL promise.

## Verification

- On the Mac: `dns-sd -B _workplacesim._tcp .` (Linux: `avahi-browse -r
  _workplacesim._tcp`) lists the running instance within a few seconds.
- `curl -sI http://workplacesim.local:4317/` returns `200 OK`.
- Open `http://workplacesim.local:4317/` in a browser — same scene the Pi
  renders to HDMI.
- Drive it from Claude Code:

  ```sh
  export WORKPLACESIM_URL=http://workplacesim.local:4317
  ```

  Start a Claude session — subagent sims should appear on the Pi's HDMI
  output.

## Post-deploy

```sh
ssh pi@workplacesim.local journalctl -u workplacesim -f       # tail logs
ssh pi@workplacesim.local systemctl status workplacesim       # service state
./deploy/install.sh pi@workplacesim.local --status-only       # both, in one shot
```

## Driving it from the Mac

Claude Code hook POSTs target `http://127.0.0.1:4317` by default. Point
them at the Pi:

```sh
export WORKPLACESIM_URL=http://workplacesim.local:4317
```

Put that in your shell rc so every Claude Code session reaches the Pi.
`install.sh` prints this exact line at the end of a successful deploy,
using whatever hostname the Pi ended up with.

If `.local` resolution is blocked on your LAN (some enterprise networks
filter mDNS), fall back to the Pi's IP:
`export WORKPLACESIM_URL=http://<ip>:4317`.

## Browser debug

The server embeds `public/index.html` and `public/main.js` and serves them
on `/`. Open `http://workplacesim.local:4317/` on your Mac to watch the
same scene Phaser renders, alongside what the Pi is drawing to HDMI. SSE
lives at `/events`.

## Live tuning via `/config`

Open `http://<pi>:4317/config` from a laptop on the same network. The page
shows server status (uptime, active sims, event rate, detected fb
geometry) and live-editable settings for sim motion, effect density,
lifecycle TTLs, and display (window size + fullscreen on the desktop
build; framebuffer info read-only on the Pi).

Changes persist to a JSON file resolved from (in order):
`$WORKPLACESIM_CONFIG_PATH`, `$XDG_CONFIG_HOME/workplacesim/config.json`
or `$HOME/.config/workplacesim/config.json`, then
`./workplacesim-config.json` next to the binary.

Out-of-range values clamp to safe bounds and bad edits fall back to
defaults without crashing the service — the `/config` page remains
reachable in every case so a faulty change can always be reverted.
`POST /api/config/reset` (or the Reset button) restores every field.

Fields that need the service to bounce before they apply (window size /
fullscreen on the fb build) are tagged `[restart]` in the form; the
`Restart service` button and `POST /api/restart` exit the process, and
systemd's `Restart=always` brings it back within ~1 second.

## Non-root operation (stretch)

The default unit runs as `root` because the Pi 1 appliance doesn't benefit
from least-privilege and both `/dev/fb0` and the VT ioctls require elevated
access out of the box. If you want to drop privileges:

1. `sudo usermod -a -G video,tty pi`
2. Edit `/etc/systemd/system/workplacesim.service` — change `User=root` to
   `User=pi`.
3. `sudo systemctl daemon-reload && sudo systemctl restart workplacesim`.

If the service fails with EACCES on `/dev/fb0` or the VT ioctls, the `pi`
user's supplementary groups aren't in effect yet — log out and back in, or
reboot.

## Uninstall

```sh
sudo systemctl disable --now workplacesim
sudo rm -- /usr/local/bin/workplacesim /etc/systemd/system/workplacesim.service
sudo systemctl daemon-reload
```

## Troubleshooting

- **Blank screen.** Framebuffer depth mismatch. `journalctl -u
  workplacesim` will name the variable and its size — fix
  `framebuffer_depth=32` in `/boot/firmware/config.txt`, reboot.
- **getty stealing tty1.** `systemctl is-active getty@tty1` should report
  `inactive`. `install.sh` disables `getty@tty1` automatically; if it's
  somehow back, `sudo systemctl stop getty@tty1.service` and restart
  workplacesim. The service unit also declares
  `Conflicts=getty@tty1.service` as a backstop.
- **avahi-daemon conflict.** If `avahi-daemon` is running (e.g. from a
  prior deploy or an `apt install`), it collides with the in-binary mDNS
  responder. Symptom: `dns-sd -B _workplacesim._tcp .` lists two records,
  or none at all. `install.sh` disables `avahi-daemon` on deploy when it
  finds it active; if it sneaks back, `sudo systemctl disable --now
  avahi-daemon` and restart workplacesim.
- **Unexpected resolution.** The renderer logs the framebuffer mode from
  `FBIOGET_VSCREENINFO` on startup (e.g.
  `fb: 1024x768 Rgb565 stride=2048 fit=1024x576@(0,96)`). Any resolution
  is fine — the scaler preserves aspect ratio with pillarbox/letterbox.
  Only force `hdmi_group`/`hdmi_mode` if you specifically want a different
  mode.
- **Hook POSTs not landing.** Verify `WORKPLACESIM_URL` on the Mac and
  that the Pi's port 4317 is reachable (`curl
  http://workplacesim.local:4317/api/agents` should return
  `{"agents":[...]}`). Check LAN firewall rules.
- **Service flapping.** `Restart=always, RestartSec=2` will keep
  restarting on crash. `journalctl -u workplacesim -n 200` for the panic
  trail.
