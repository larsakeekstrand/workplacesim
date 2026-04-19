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
Pi, and `ssh`es in to enable and restart the service.

Requires `cross`, `docker`, `ssh`, `scp` on the host. Install cross with
`cargo install cross`. Docker must be running (cross uses it for the arm
toolchain image).

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
export WORKPLACESIM_URL=http://<host>:4317
```

Put that in your shell rc so every Claude Code session reaches the Pi.

## Browser debug

The server embeds `public/index.html` and `public/main.js` and serves them on
`/`. Open `http://<host>:4317/` on your Mac to watch the same scene Phaser
renders, alongside what the Pi is drawing to HDMI. SSE lives at `/events`.

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
