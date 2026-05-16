#!/usr/bin/env bash
set -euo pipefail

# Cross-build workplacesim for Pi 1 (armv6, hard-float) and install it as an
# OpenRC service over SSH on Alpine Linux. mDNS is advertised from the binary
# itself, so no avahi/systemd-resolved is installed. Target must already have
# sshd running and the invoking user's key in authorized_keys.

usage() {
    printf 'usage: %s <user>@<host> [--status-only] [--hostname <name>] [--skip-hostname]\n' "${0##*/}" >&2
    printf '  <user>@<host>     target Pi, e.g. root@raspberrypi.local\n' >&2
    printf '  --status-only     skip build+copy, just show service status + recent logs\n' >&2
    printf '  --hostname <name> set Pi hostname so clients reach it at <name>.local (default: workplacesim)\n' >&2
    printf '  --skip-hostname   leave the Pi hostname untouched\n' >&2
    exit 2
}

if [[ $# -lt 1 ]]; then
    usage
fi

TARGET="$1"
shift || true

STATUS_ONLY=0
DESIRED_HOSTNAME="workplacesim"
SKIP_HOSTNAME=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --status-only) STATUS_ONLY=1 ;;
        --hostname)
            shift || { printf 'error: --hostname requires a value\n' >&2; usage; }
            DESIRED_HOSTNAME="$1"
            ;;
        --skip-hostname) SKIP_HOSTNAME=1 ;;
        -h|--help) usage ;;
        *) printf 'unknown argument: %s\n' "$1" >&2; usage ;;
    esac
    shift
done

if [[ "${TARGET}" != *"@"* ]]; then
    printf 'error: target must be <user>@<host>, got: %s\n' "${TARGET}" >&2
    usage
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
CRATE_DIR="$(cd -- "${SCRIPT_DIR}/.." &>/dev/null && pwd)"
REPO_ROOT="$(cd -- "${CRATE_DIR}/../.." &>/dev/null && pwd)"
INITD_FILE="${SCRIPT_DIR}/workplacesim.initd"
BIN_REL="target/arm-unknown-linux-gnueabihf/release/workplacesim"
BIN_ABS="${CRATE_DIR}/${BIN_REL}"

if [[ "${STATUS_ONLY}" -eq 1 ]]; then
    ssh "${TARGET}" 'rc-service workplacesim status || true; echo; tail -n 30 /var/log/workplacesim.log /var/log/workplacesim.err 2>/dev/null || true'
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

if ! [[ -f "${INITD_FILE}" ]]; then
    printf 'error: init script not found at %s\n' "${INITD_FILE}" >&2
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
scp -q -- "${INITD_FILE}" "${TARGET}:/tmp/workplacesim.initd"

ssh "${TARGET}" "DESIRED_HOSTNAME='${DESIRED_HOSTNAME}' SKIP_HOSTNAME='${SKIP_HOSTNAME}' sh -se" <<'REMOTE'
set -eu

# sudo may not exist on a stock Alpine image; fall back to running as root
# when already root (common on Alpine rpi tarball, which ships with a root
# login and no sudo).
if [ "$(id -u)" -eq 0 ]; then
    SUDO=""
else
    if command -v sudo >/dev/null 2>&1; then
        SUDO="sudo"
    elif command -v doas >/dev/null 2>&1; then
        SUDO="doas"
    else
        echo "error: need root (sudo/doas not found and not running as root)" >&2
        exit 1
    fi
fi

# OpenRC and busybox-openrc ship in the Alpine rpi tarball. Install only if
# rc-service is missing (covers stripped-down custom images).
if ! command -v rc-service >/dev/null 2>&1; then
    $SUDO apk update
    $SUDO apk add --no-interactive openrc busybox-openrc
fi

# Hostname: /etc/hostname on Alpine is the full name, no FQDN dance.
if [ "${SKIP_HOSTNAME}" != "1" ]; then
    current="$(hostname)"
    if [ "${current}" != "${DESIRED_HOSTNAME}" ]; then
        echo "${DESIRED_HOSTNAME}" | $SUDO tee /etc/hostname >/dev/null
        $SUDO hostname -F /etc/hostname
        # Rewrite the loopback line in /etc/hosts if it names the old host so
        # local name resolution doesn't drift.
        if [ -f /etc/hosts ] && grep -qE "^127\.0\.1\.1[[:space:]]" /etc/hosts; then
            $SUDO sed -i.bak "s/^127\.0\.1\.1.*/127.0.1.1\t${DESIRED_HOSTNAME}/" /etc/hosts
        elif [ -f /etc/hosts ] && grep -qE "^127\.0\.0\.1[[:space:]]+${current}([[:space:]]|$)" /etc/hosts; then
            $SUDO sed -i.bak "s/\b${current}\b/${DESIRED_HOSTNAME}/g" /etc/hosts
        fi
        printf 'hostname: %s -> %s\n' "${current}" "${DESIRED_HOSTNAME}"
    fi
fi

# Free tty1 from getty so the renderer can take the VT. Alpine's inittab
# line is typically `tty1::respawn:/sbin/getty 38400 tty1`. Comment it
# idempotently — if the line is already commented, leave it alone.
if [ -f /etc/inittab ] && grep -qE '^[[:space:]]*tty1::respawn' /etc/inittab; then
    $SUDO sed -i.bak -E 's|^([[:space:]]*tty1::respawn.*)$|#\1|' /etc/inittab
    # Ask init to re-read inittab so the getty is not respawned after we
    # kill it below.
    $SUDO kill -HUP 1 || true
fi
# Kill any live getty sitting on tty1 so our service can grab the VT on
# first start. `pkill -f` matches the full arg list; fall back to busybox
# `killall` if pkill is absent.
if command -v pkill >/dev/null 2>&1; then
    $SUDO pkill -f 'getty.*tty1' 2>/dev/null || true
else
    $SUDO killall -q getty 2>/dev/null || true
fi

$SUDO install -m 0755 /tmp/workplacesim.new /usr/local/bin/workplacesim
$SUDO install -m 0755 /tmp/workplacesim.initd /etc/init.d/workplacesim

# rc-update is idempotent — re-adding is a no-op. Restart covers both the
# fresh-install and redeploy cases.
$SUDO rc-update add workplacesim default >/dev/null 2>&1 || $SUDO rc-update add workplacesim default
$SUDO rc-service workplacesim restart

sleep 1
$SUDO rc-service workplacesim status || true
rm -f -- /tmp/workplacesim.new /tmp/workplacesim.initd
REMOTE

printf 'deployed workplacesim to %s\n' "${TARGET}"

if [[ "${SKIP_HOSTNAME}" == "1" ]]; then
    ADVERTISED_HOST="$(ssh "${TARGET}" 'hostname' 2>/dev/null || echo "${DESIRED_HOSTNAME}")"
else
    ADVERTISED_HOST="${DESIRED_HOSTNAME}"
fi

printf '\nReady at: http://%s.local:4317\n' "${ADVERTISED_HOST}"
printf '(first-run mDNS advertisement may take a few seconds to propagate.)\n'
printf '\nPoint Claude Code hooks at the Pi by adding this to your shell rc:\n'
printf '  export WORKPLACESIM_URL=http://%s.local:4317\n' "${ADVERTISED_HOST}"
