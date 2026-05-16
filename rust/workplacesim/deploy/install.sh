#!/usr/bin/env bash
set -euo pipefail

# Cross-build workplacesim for Pi 1 (armv6, hard-float) and install it as a
# systemd service on Raspberry Pi OS Lite (Bookworm) over SSH. Assumes the Pi
# was provisioned with rpi-imager's Advanced Options: SSH enabled with the
# invoking user's pubkey authorized for the login user, hostname + wifi +
# locale + timezone preseeded. This script does not touch any of that.

usage() {
    cat >&2 <<EOF
usage: ${0##*/} <target> [--status-only] [--hostname <name>] [--skip-hostname] [--user <user>]
  <target>          target Pi as either "user@host" or bare "host"
                    bare "host" is prefixed with --user (default: pi)
                    examples: pi@workplacesim.local
                              workplacesim.local            (uses pi@)
                              workplacesim.local --user me  (uses me@)
  --status-only     skip build+copy, just show service status + recent logs
  --hostname <name> set the Pi's hostname before installing the service
                    (default: do NOT touch hostname; trust Pi Imager's setting)
  --skip-hostname   alias for the default; left in for backward compat
  --user <user>     SSH login user when <target> is bare host (default: pi)
EOF
    exit 2
}

if [[ $# -lt 1 ]]; then
    usage
fi

TARGET="$1"
shift || true

STATUS_ONLY=0
DESIRED_HOSTNAME=""
SET_HOSTNAME=0
SKIP_HOSTNAME=0
DEFAULT_USER="pi"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --status-only) STATUS_ONLY=1 ;;
        --hostname)
            shift || { printf 'error: --hostname requires a value\n' >&2; usage; }
            DESIRED_HOSTNAME="$1"
            SET_HOSTNAME=1
            ;;
        --skip-hostname) SKIP_HOSTNAME=1 ;;
        --user)
            shift || { printf 'error: --user requires a value\n' >&2; usage; }
            DEFAULT_USER="$1"
            ;;
        -h|--help) usage ;;
        *) printf 'unknown argument: %s\n' "$1" >&2; usage ;;
    esac
    shift
done

# If --skip-hostname was passed alongside --hostname, --skip-hostname wins
# (matches old behaviour: be conservative).
if [[ "${SKIP_HOSTNAME}" -eq 1 ]]; then
    SET_HOSTNAME=0
fi

# Normalise target: accept "user@host" verbatim, otherwise prepend "${DEFAULT_USER}@".
if [[ "${TARGET}" != *"@"* ]]; then
    TARGET="${DEFAULT_USER}@${TARGET}"
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
CRATE_DIR="$(cd -- "${SCRIPT_DIR}/.." &>/dev/null && pwd)"
REPO_ROOT="$(cd -- "${CRATE_DIR}/../.." &>/dev/null && pwd)"
UNIT_FILE="${SCRIPT_DIR}/workplacesim.service"
BIN_REL="target/arm-unknown-linux-gnueabihf/release/workplacesim"
BIN_ABS="${CRATE_DIR}/${BIN_REL}"

if [[ "${STATUS_ONLY}" -eq 1 ]]; then
    ssh "${TARGET}" 'sudo systemctl status --no-pager --lines 10 workplacesim.service || true; echo; sudo journalctl -u workplacesim -n 30 --no-pager || true'
    exit 0
fi

for tool in cross docker ssh scp; do
    if ! command -v "${tool}" >/dev/null 2>&1; then
        case "${tool}" in
            cross)  printf 'error: %s not on PATH. install with: cargo install cross\n' "${tool}" >&2 ;;
            docker) printf 'error: %s not on PATH. Docker is required by cross for the arm-unknown-linux-gnueabihf target\n' "${tool}" >&2 ;;
            *)      printf 'error: %s not on PATH\n' "${tool}" >&2 ;;
        esac
        exit 1
    fi
done

if ! [[ -f "${UNIT_FILE}" ]]; then
    printf 'error: service unit not found at %s\n' "${UNIT_FILE}" >&2
    exit 1
fi

export WORKPLACESIM_REPO="${REPO_ROOT}"
printf 'building for arm-unknown-linux-gnueabihf (repo=%s)\n' "${WORKPLACESIM_REPO}"

(
    cd -- "${CRATE_DIR}"
    RUSTFLAGS='-C target-cpu=arm1176jzf-s' \
        cross build \
            --target arm-unknown-linux-gnueabihf \
            --release \
            --features fb \
            --no-default-features
)

if ! [[ -x "${BIN_ABS}" ]]; then
    printf 'error: expected binary not found at %s\n' "${BIN_ABS}" >&2
    exit 1
fi

BIN_SIZE="$(wc -c <"${BIN_ABS}" | tr -d '[:space:]')"
printf 'built %s bytes; copying to %s\n' "${BIN_SIZE}" "${TARGET}"

scp -q -- "${BIN_ABS}" "${TARGET}:/tmp/workplacesim.new"
scp -q -- "${UNIT_FILE}" "${TARGET}:/tmp/workplacesim.service"

ssh "${TARGET}" "DESIRED_HOSTNAME='${DESIRED_HOSTNAME}' SET_HOSTNAME='${SET_HOSTNAME}' bash -se" <<'REMOTE'
set -euo pipefail

if [[ "${SET_HOSTNAME}" == "1" && -n "${DESIRED_HOSTNAME}" ]]; then
    current="$(hostname)"
    if [[ "${current}" != "${DESIRED_HOSTNAME}" ]]; then
        sudo hostnamectl set-hostname "${DESIRED_HOSTNAME}"
        # /etc/hosts 127.0.1.1 line is what hostname -> IP resolution leans on;
        # rewrite it atomically so a reboot isn't required for sudo to stop
        # warning about unresolvable hostnames.
        if grep -q '^127\.0\.1\.1' /etc/hosts; then
            sudo sed -i.bak "s/^127\.0\.1\.1.*/127.0.1.1\t${DESIRED_HOSTNAME}/" /etc/hosts
        else
            echo -e "127.0.1.1\t${DESIRED_HOSTNAME}" | sudo tee -a /etc/hosts >/dev/null
        fi
        printf 'hostname: %s -> %s\n' "${current}" "${DESIRED_HOSTNAME}"
    fi
fi

# Install binary + unit.
sudo install -m 0755 /tmp/workplacesim.new /usr/local/bin/workplacesim
sudo install -m 0644 /tmp/workplacesim.service /etc/systemd/system/workplacesim.service

# Belt-and-suspenders: the unit declares Conflicts=getty@tty1.service which
# stops it on activation, but disabling outright keeps it from coming back
# on the next boot and racing for the framebuffer.
sudo systemctl disable --now getty@tty1.service || true

# If avahi-daemon is installed and enabled, it will conflict with the
# binary's in-process mDNS responder (both binding UDP/5353). Disable it
# but do not purge — the user may want it back for other reasons.
if systemctl list-unit-files avahi-daemon.service >/dev/null 2>&1 \
   && systemctl is-enabled avahi-daemon.service >/dev/null 2>&1; then
    printf 'warning: avahi-daemon is enabled; disabling so it does not collide with the in-binary mDNS responder\n' >&2
    sudo systemctl disable --now avahi-daemon.service || true
    sudo systemctl disable --now avahi-daemon.socket || true
fi

sudo systemctl daemon-reload
sudo systemctl enable --now workplacesim.service
sleep 1
sudo systemctl status --no-pager --lines 10 workplacesim.service || true
echo
sudo journalctl -u workplacesim -n 10 --no-pager || true

rm -f -- /tmp/workplacesim.new /tmp/workplacesim.service
REMOTE

printf 'deployed workplacesim to %s\n' "${TARGET}"

if [[ "${SET_HOSTNAME}" == "1" && -n "${DESIRED_HOSTNAME}" ]]; then
    ADVERTISED_HOST="${DESIRED_HOSTNAME}"
else
    ADVERTISED_HOST="$(ssh "${TARGET}" 'hostname' 2>/dev/null || echo workplacesim)"
fi

printf '\nPoint Claude Code hooks at the Pi by adding this to your shell rc:\n'
printf '  export WORKPLACESIM_URL=http://%s.local:4317\n' "${ADVERTISED_HOST}"
