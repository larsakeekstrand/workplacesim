# workplacesim Pi deploy

Cross-build, copy, and run workplacesim on a Raspberry Pi 1 (armv6, hard-float)
as a systemd service that owns `tty1` and draws to `/dev/fb0`.

## Pi prep

Edit `/boot/config.txt` (or `/boot/firmware/config.txt` on newer images):

- `framebuffer_depth=32` or `framebuffer_depth=16` — renderer auto-detects
  XRGB8888 (32bpp) and RGB565 (16bpp). Other depths are rejected with a
  message pointing back at this knob.
- `hdmi_force_hotplug=1` — keep HDMI output on even without EDID (headless boot).
- `disable_overscan=1` — no black border around the canvas.
- Optional: `hdmi_group=2`, `hdmi_mode=82` to force 1080p if autodetect guesses wrong.
  The renderer scales aspect-preserving nearest-neighbour to any detected
  resolution with black pillar/letterbox — you don't have to force 1280×720.

Enable SSH (`sudo raspi-config` → Interface Options → SSH) and connect the Pi
to the same LAN as your Mac.

## Deploy from the Mac

```sh
cd rust/workplacesim
./deploy/install.sh pi@<host>
```

The script runs `cross build --target arm-unknown-linux-gnueabihf --release
--features fb --no-default-features`, `scp`s the binary + service unit to the
Pi, and `ssh`es in to enable and restart the service. It also installs
`avahi-daemon` (if missing), renames the Pi to `workplacesim`, and publishes
an mDNS-SD service record so the Pi is reachable at `workplacesim.local` on
the LAN without any per-client discovery.

Requires `cross`, `docker`, `ssh`, `scp` on the host. Install cross with
`cargo install cross`. Docker must be running (cross uses it for the arm
toolchain image).

### Install flags

- `--hostname <name>` — use a hostname other than `workplacesim`. Clients
  then reach the Pi at `<name>.local:4317`.
- `--skip-hostname` — leave the existing Pi hostname alone. The avahi service
  record still gets installed, so the Pi shows up as
  `workplacesim on <current-hostname>` in Bonjour browsers and is reachable
  at `<current-hostname>.local:4317`.
- `--status-only` — skip build + copy; just print service status and logs.

### Multiple Pis on one LAN

Give each Pi a distinct hostname (`--hostname wpsim-lab`, `--hostname
wpsim-desk`). Without that, avahi resolves name collisions by suffixing the
second to `workplacesim-2.local`, which defeats the predictable-URL promise.

## Post-deploy

```sh
ssh pi@<host> journalctl -u workplacesim -f       # tail logs
ssh pi@<host> systemctl status workplacesim       # service state
./deploy/install.sh pi@<host> --status-only       # both, in one shot
```

## Driving it from the Mac

Claude Code hook POSTs target `http://127.0.0.1:4317` by default. Point them
at the Pi:

```sh
export WORKPLACESIM_URL=http://workplacesim.local:4317
```

Put that in your shell rc so every Claude Code session reaches the Pi.
`install.sh` prints this exact line at the end of a successful deploy, using
whatever hostname the Pi ended up with.

If `.local` resolution is blocked on your LAN (some enterprise networks
filter mDNS), fall back to the Pi's IP:
`export WORKPLACESIM_URL=http://<ip>:4317`.

## Browser debug

The server embeds `public/index.html` and `public/main.js` and serves them on
`/`. Open `http://workplacesim.local:4317/` on your Mac to watch the same
scene Phaser renders, alongside what the Pi is drawing to HDMI. SSE lives at
`/events`.

To confirm the mDNS advertisement from the Mac:
`dns-sd -B _workplacesim._tcp` should list the running instance.

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

## Uninstall

```sh
sudo systemctl disable --now workplacesim
sudo rm -- /usr/local/bin/workplacesim /etc/systemd/system/workplacesim.service
sudo systemctl daemon-reload
```

## Troubleshooting

- **Blank screen.** Framebuffer depth mismatch. `journalctl -u workplacesim`
  will name the variable and its size — fix `framebuffer_depth=32` in
  `config.txt`, reboot.
- **getty stealing tty1.** `systemctl is-active getty@tty1` should report
  `inactive`. The service unit declares `Conflicts=getty@tty1.service`; if
  it isn't honoured, `sudo systemctl stop getty@tty1.service` and restart
  workplacesim.
- **Unexpected resolution.** The renderer logs the framebuffer mode from
  `FBIOGET_VSCREENINFO` on startup (e.g.
  `fb: 1024x768 Rgb565 stride=2048 fit=1024x576@(0,96)`). Any resolution is
  fine — the scaler preserves aspect ratio with pillarbox/letterbox. Only
  force `hdmi_group`/`hdmi_mode` if you specifically want a different mode.
- **Hook POSTs not landing.** Verify `WORKPLACESIM_URL` on the Mac and that
  the Pi's port 4317 is reachable (`curl http://<host>:4317/api/agents`
  should return `{"agents":[...]}`). Check LAN firewall rules.
- **Service flapping.** `Restart=always, RestartSec=2` will keep restarting
  on crash. `journalctl -u workplacesim -n 200` for the panic trail.
