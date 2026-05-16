#!/usr/bin/env bash
set -euo pipefail

# Cross-build workplacesim for Pi 1 (armv6, hard-float) and install it as a
# systemd service over SSH. Target must already have sshd running and the
# invoking user's key in authorized_keys.

usage() {
    printf 'usage: %s <user>@<host> [--status-only] [--hostname <name>] [--skip-hostname]\n' "${0##*/}" >&2
    printf '  <user>@<host>     target Pi, e.g. pi@raspberrypi.local\n' >&2
    printf '  --status-only     skip build+copy, just show service status + recent logs\n' >&2
    printf '  --hostname <name> set Pi hostname so clients reach it at <name>.local (default: workplacesim)\n' >&2
    printf '  --skip-hostname   leave the Pi hostname untouched; still install avahi service file\n' >&2
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
UNIT_FILE="${SCRIPT_DIR}/workplacesim.service"
AVAHI_FILE="${SCRIPT_DIR}/workplacesim.avahi-service"
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

if ! [[ -f "${AVAHI_FILE}" ]]; then
    printf 'error: avahi service file not found at %s\n' "${AVAHI_FILE}" >&2
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
scp -q -- "${AVAHI_FILE}" "${TARGET}:/tmp/workplacesim.avahi-service"

ssh "${TARGET}" "DESIRED_HOSTNAME='${DESIRED_HOSTNAME}' SKIP_HOSTNAME='${SKIP_HOSTNAME}' bash -se" <<'REMOTE'
set -euo pipefail

if ! dpkg -s avahi-daemon >/dev/null 2>&1; then
    sudo apt-get update -qq
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y avahi-daemon
fi

if [[ "${SKIP_HOSTNAME}" != "1" ]]; then
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

sudo install -m 0644 /tmp/workplacesim.avahi-service /etc/avahi/services/workplacesim.service
sudo systemctl enable --now avahi-daemon
# Must be restart, not reload: SIGHUP re-reads /etc/avahi/services/* but does
# not re-read the system hostname. After a hostnamectl change avahi keeps
# advertising the previous name until the daemon actually re-execs.
sudo systemctl restart avahi-daemon

sudo install -m 0755 /tmp/workplacesim.new /usr/local/bin/workplacesim
sudo install -m 0644 /tmp/workplacesim.service /etc/systemd/system/workplacesim.service
sudo systemctl daemon-reload
sudo systemctl enable workplacesim.service
sudo systemctl restart workplacesim.service
sleep 1
sudo systemctl status --no-pager --lines 10 workplacesim.service || true
rm -f -- /tmp/workplacesim.new /tmp/workplacesim.service /tmp/workplacesim.avahi-service
REMOTE

printf 'deployed workplacesim to %s\n' "${TARGET}"

if [[ "${SKIP_HOSTNAME}" == "1" ]]; then
    ADVERTISED_HOST="$(ssh "${TARGET}" 'hostname' 2>/dev/null || echo "${DESIRED_HOSTNAME}")"
else
    ADVERTISED_HOST="${DESIRED_HOSTNAME}"
fi

printf '\nPoint Claude Code hooks at the Pi by adding this to your shell rc:\n'
printf '  export WORKPLACESIM_URL=http://%s.local:4317\n' "${ADVERTISED_HOST}"
