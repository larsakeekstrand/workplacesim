#!/usr/bin/env bash
set -euo pipefail

# Cross-build workplacesim for Pi 1 (armv6, hard-float) and install it as a
# systemd service over SSH. Target must already have sshd running and the
# invoking user's key in authorized_keys.

usage() {
    printf 'usage: %s <user>@<host> [--status-only]\n' "${0##*/}" >&2
    printf '  <user>@<host>   target Pi, e.g. pi@raspberrypi.local\n' >&2
    printf '  --status-only   skip build+copy, just show service status + recent logs\n' >&2
    exit 2
}

if [[ $# -lt 1 ]]; then
    usage
fi

TARGET="$1"
shift || true

STATUS_ONLY=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --status-only) STATUS_ONLY=1 ;;
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

ssh "${TARGET}" 'bash -se' <<'REMOTE'
set -euo pipefail
sudo install -m 0755 /tmp/workplacesim.new /usr/local/bin/workplacesim
sudo install -m 0644 /tmp/workplacesim.service /etc/systemd/system/workplacesim.service
sudo systemctl daemon-reload
sudo systemctl enable workplacesim.service
sudo systemctl restart workplacesim.service
sleep 1
sudo systemctl status --no-pager --lines 10 workplacesim.service || true
rm -f -- /tmp/workplacesim.new /tmp/workplacesim.service
REMOTE

printf 'deployed workplacesim to %s\n' "${TARGET}"
