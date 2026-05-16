#!/usr/bin/env bash
set -euo pipefail

# Bake a flashable Raspberry Pi SD-card image containing Alpine Linux, the
# workplacesim binary, the OpenRC service, and (optionally) openssh +
# wpa_supplicant + wifi firmware + your SSH public key — all pre-extracted
# into the apkovl so the Pi comes up correct on first boot with no manual
# setup.
#
# After `dd`-ing the produced image to an SD card, boot the Pi. workplacesim
# starts on HDMI, mDNS is live at <hostname>.local:4317, and (if SSH was
# baked in) `ssh root@<hostname>.local` works immediately with your key.
#
# Loop devices are Linux-only. macOS users can still run `--dry-run` to
# review the plan. A full bake requires Linux (bare metal, VM, or a
# privileged `alpine:3.20` Docker container — the latter is the recommended
# workflow on macOS).

usage() {
    cat >&2 <<USAGE
usage: ${0##*/} [OPTIONS]

Core options:
  --dry-run                 Print the plan without touching loop devices,
                            mounting anything, or writing to the image.
                            Works on macOS (planning only).
  --output PATH             Output image path. Default: workplacesim-pi.img
                            in the current working directory.
  --alpine-version VER      Alpine release to fetch. Default: 3.20.3.
  --hostname NAME           Pi hostname. Default: workplacesim.

Wifi options (both required to enable wifi; otherwise wlan0 isn't configured):
  --wifi-ssid SSID          Wifi network name. Also reads WIFI_SSID env.
  --wifi-psk PSK            Wifi password.     Also reads WIFI_PSK env.

SSH options (any of these triggers openssh install + sshd in default runlevel):
  --ssh-pubkey FILE         Path to a public key file (e.g. id_ed25519.pub).
                            Installed as /root/.ssh/authorized_keys. Also
                            reads SSH_PUBKEY env.
  --no-ssh                  Force-disable SSH even if a pubkey is provided.

Networking (defaults to DHCP on eth0):
  --static-ip ADDR/PREFIX   Static IPv4 for eth0, e.g. 192.168.2.10/24.
                            Useful for direct Mac<->Pi cable setups where
                            Internet Sharing assigns 192.168.2.1 to the Mac.
                            Also reads STATIC_IP env.
  --gateway ADDR            Default gateway. Only used with --static-ip.
                            Also reads GATEWAY env.

  -h, --help                Show this message.

Environment:
  XDG_CACHE_HOME            Alpine tarball + sha256 are cached under
                            \$XDG_CACHE_HOME/workplacesim/ (default
                            ~/.cache/workplacesim/).
USAGE
    exit 2
}

# -----------------------------------------------------------------------------
# Argument parsing
# -----------------------------------------------------------------------------

DRY_RUN=0
OUTPUT=""
ALPINE_VERSION="3.20.3"
HOSTNAME_VAL="workplacesim"
WIFI_SSID="${WIFI_SSID:-}"
WIFI_PSK="${WIFI_PSK:-}"
SSH_PUBKEY="${SSH_PUBKEY:-}"
DISABLE_SSH=0
STATIC_IP="${STATIC_IP:-}"
GATEWAY="${GATEWAY:-}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
        --output)
            shift || { printf 'error: --output requires a value\n' >&2; usage; }
            OUTPUT="$1"
            ;;
        --alpine-version)
            shift || { printf 'error: --alpine-version requires a value\n' >&2; usage; }
            ALPINE_VERSION="$1"
            ;;
        --hostname)
            shift || { printf 'error: --hostname requires a value\n' >&2; usage; }
            HOSTNAME_VAL="$1"
            ;;
        --wifi-ssid)
            shift || { printf 'error: --wifi-ssid requires a value\n' >&2; usage; }
            WIFI_SSID="$1"
            ;;
        --wifi-psk)
            shift || { printf 'error: --wifi-psk requires a value\n' >&2; usage; }
            WIFI_PSK="$1"
            ;;
        --ssh-pubkey)
            shift || { printf 'error: --ssh-pubkey requires a value\n' >&2; usage; }
            SSH_PUBKEY="$1"
            ;;
        --no-ssh) DISABLE_SSH=1 ;;
        --static-ip)
            shift || { printf 'error: --static-ip requires a value\n' >&2; usage; }
            STATIC_IP="$1"
            ;;
        --gateway)
            shift || { printf 'error: --gateway requires a value\n' >&2; usage; }
            GATEWAY="$1"
            ;;
        -h|--help) usage ;;
        *) printf 'unknown argument: %s\n' "$1" >&2; usage ;;
    esac
    shift
done

if [[ -z "${OUTPUT}" ]]; then
    OUTPUT="$(pwd)/workplacesim-pi.img"
fi

# Validate wifi flags (both or neither)
if [[ -n "${WIFI_SSID}" && -z "${WIFI_PSK}" ]] || [[ -z "${WIFI_SSID}" && -n "${WIFI_PSK}" ]]; then
    printf 'error: --wifi-ssid and --wifi-psk must be specified together\n' >&2
    exit 2
fi
WIFI_ENABLED=0
[[ -n "${WIFI_SSID}" ]] && WIFI_ENABLED=1

SSH_ENABLED=0
if [[ "${DISABLE_SSH}" -eq 0 ]]; then
    [[ -n "${SSH_PUBKEY}" ]] && SSH_ENABLED=1
fi
if [[ "${SSH_ENABLED}" -eq 1 && ! -f "${SSH_PUBKEY}" ]]; then
    printf 'error: --ssh-pubkey path does not exist: %s\n' "${SSH_PUBKEY}" >&2
    exit 2
fi

# Validate static IP (rough syntactic check; the resulting interfaces file
# either works or it doesn't — leaving to the Pi side to surface real errors).
if [[ -n "${STATIC_IP}" && ! "${STATIC_IP}" =~ ^[0-9.]+/[0-9]+$ ]]; then
    printf 'error: --static-ip must be in CIDR form like 192.168.2.10/24\n' >&2
    exit 2
fi

# -----------------------------------------------------------------------------
# Paths
# -----------------------------------------------------------------------------

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
CRATE_DIR="$(cd -- "${SCRIPT_DIR}/.." &>/dev/null && pwd)"
REPO_ROOT="$(cd -- "${CRATE_DIR}/../.." &>/dev/null && pwd)"

OVERLAY_DIR="${SCRIPT_DIR}/image-overlay"
INITD_FILE="${SCRIPT_DIR}/workplacesim.initd"

# Prefer the musl-static binary (the only one that actually runs on Alpine
# without a glibc shim). Fall back to the glibc one for back-compat with
# existing target dirs; warn if we end up using it because it will not run.
BIN_MUSL_REL="target-pi-musl/arm-unknown-linux-musleabihf/release/workplacesim"
BIN_GLIBC_REL="target/arm-unknown-linux-gnueabihf/release/workplacesim"
BIN_MUSL_ABS="${CRATE_DIR}/${BIN_MUSL_REL}"
BIN_GLIBC_ABS="${CRATE_DIR}/${BIN_GLIBC_REL}"

CACHE_DIR="${XDG_CACHE_HOME:-${HOME}/.cache}/workplacesim"
ALPINE_MINOR="${ALPINE_VERSION%.*}"
ALPINE_TARBALL="alpine-rpi-${ALPINE_VERSION}-armhf.tar.gz"
ALPINE_URL="https://dl-cdn.alpinelinux.org/alpine/v${ALPINE_MINOR}/releases/armhf/${ALPINE_TARBALL}"
ALPINE_SHA_URL="${ALPINE_URL}.sha256"
ALPINE_TARBALL_PATH="${CACHE_DIR}/${ALPINE_TARBALL}"
ALPINE_SHA_PATH="${CACHE_DIR}/${ALPINE_TARBALL}.sha256"

ALPINE_REPO_MAIN="http://dl-cdn.alpinelinux.org/alpine/v${ALPINE_MINOR}/main"
ALPINE_REPO_COMMUNITY="http://dl-cdn.alpinelinux.org/alpine/v${ALPINE_MINOR}/community"

IMAGE_SIZE_BYTES=536870912   # 512 MiB

# Cleanup state (set by real run; harmless in dry-run)
LOOP_DEV=""
MOUNT_DIR=""
STAGING_DIR=""
PKG_STAGING=""
PKG_CACHE=""

log()  { printf '==> %s\n' "$*"; }
warn() { printf 'warn: %s\n' "$*" >&2; }
die()  { printf 'error: %s\n' "$*" >&2; exit 1; }

# -----------------------------------------------------------------------------
# Cleanup trap
# -----------------------------------------------------------------------------

cleanup() {
    local rc=$?
    if [[ -n "${MOUNT_DIR}" && -d "${MOUNT_DIR}" ]]; then
        if mountpoint -q "${MOUNT_DIR}" 2>/dev/null; then
            umount "${MOUNT_DIR}" 2>/dev/null || true
        fi
        rmdir "${MOUNT_DIR}" 2>/dev/null || true
    fi
    if [[ -n "${LOOP_DEV}" ]]; then
        losetup -d "${LOOP_DEV}" 2>/dev/null || true
    fi
    [[ -n "${STAGING_DIR}" ]] && rm -rf -- "${STAGING_DIR}" 2>/dev/null || true
    [[ -n "${PKG_STAGING}" ]] && rm -rf -- "${PKG_STAGING}" 2>/dev/null || true
    [[ -n "${PKG_CACHE}"   ]] && rm -rf -- "${PKG_CACHE}"   2>/dev/null || true
    exit "${rc}"
}
trap cleanup EXIT INT TERM

# -----------------------------------------------------------------------------
# Platform check
# -----------------------------------------------------------------------------

HOST_OS="$(uname -s)"
if [[ "${HOST_OS}" != "Linux" ]]; then
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        log "host is ${HOST_OS}; --dry-run is planning-only and does not require Linux"
    else
        cat >&2 <<EOF
error: mkimage.sh requires Linux (losetup + loopback mount are used to
       partition and populate the image). Detected host: ${HOST_OS}.

Options to complete a real bake:
  - Run in a privileged alpine:3.20 Docker container (recommended on macOS):
      docker run --rm --privileged -v "\$PWD":/work -w /work/rust/workplacesim \\
          alpine:3.20 sh -c '
              apk add --no-cache bash parted dosfstools util-linux e2fsprogs \\
                                 curl tar gzip coreutils openssh-keygen
              ./deploy/mkimage.sh ARGS...
          '
  - Run on a Linux VM or on the Pi itself.

You can still use --dry-run on this machine to review the plan.
EOF
        exit 2
    fi
fi

# -----------------------------------------------------------------------------
# Tool presence
# -----------------------------------------------------------------------------

REQUIRED_TOOLS=(curl sha256sum losetup parted mkfs.vfat tar mount partx)
# These are optional features — only required if the user actually enabled them.
[[ "${SSH_ENABLED}" -eq 1 ]] && REQUIRED_TOOLS+=(ssh-keygen)
[[ "${WIFI_ENABLED}" -eq 1 || "${SSH_ENABLED}" -eq 1 ]] && REQUIRED_TOOLS+=(apk)

MISSING_TOOLS=()
for tool in "${REQUIRED_TOOLS[@]}"; do
    if ! command -v "${tool}" >/dev/null 2>&1; then
        MISSING_TOOLS+=("${tool}")
    fi
done

if [[ "${#MISSING_TOOLS[@]}" -gt 0 ]]; then
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        warn "missing tools (would block a real bake): ${MISSING_TOOLS[*]}"
    else
        printf 'error: missing required tools: %s\n' "${MISSING_TOOLS[*]}" >&2
        printf '  install hints (Alpine):\n' >&2
        printf '    apk add bash parted dosfstools util-linux e2fsprogs curl tar gzip coreutils openssh-keygen\n' >&2
        printf '  install hints (Debian/Ubuntu):\n' >&2
        printf '    apt install util-linux parted dosfstools coreutils tar curl openssh-client apk-tools\n' >&2
        exit 2
    fi
fi

# -----------------------------------------------------------------------------
# Binary selection
# -----------------------------------------------------------------------------

if [[ -x "${BIN_MUSL_ABS}" ]]; then
    BIN_ABS="${BIN_MUSL_ABS}"
    log "using musl-static binary at ${BIN_MUSL_REL}"
elif [[ -x "${BIN_GLIBC_ABS}" ]]; then
    BIN_ABS="${BIN_GLIBC_ABS}"
    warn "no musl binary found; using glibc binary at ${BIN_GLIBC_REL}."
    warn "this WILL NOT run on Alpine (musl libc). Build the musl target:"
    warn "  docker run --rm -v \"\$PWD\":/work -w /work/rust/workplacesim \\"
    warn "      -e CARGO_TARGET_DIR=/work/target-pi-musl rust:alpine sh -c '"
    warn "    apk add --no-cache musl-dev build-base"
    warn "    rustup target add arm-unknown-linux-musleabihf"
    warn "    RUSTFLAGS=\"-C target-cpu=arm1176jzf-s -C linker=rust-lld -C link-self-contained=yes\" \\"
    warn "      cargo build --target arm-unknown-linux-musleabihf --release --features fb --no-default-features"
    warn "  '"
else
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        warn "no binary at ${BIN_MUSL_REL} or ${BIN_GLIBC_REL}"
    else
        die "no binary found at ${BIN_MUSL_REL} or ${BIN_GLIBC_REL}. Build the musl target first; see --help."
    fi
    BIN_ABS="${BIN_MUSL_ABS}"  # placeholder for dry-run output
fi

# -----------------------------------------------------------------------------
# Alpine tarball: download + sha256 verify
# -----------------------------------------------------------------------------

fetch_alpine() {
    log "alpine: ${ALPINE_URL}"
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        cat <<PLAN
    mkdir -p ${CACHE_DIR}
    curl -fsSIL ${ALPINE_URL} >/dev/null
    curl -fsSL -o ${ALPINE_SHA_PATH} ${ALPINE_SHA_URL}
    curl -fsSL -o ${ALPINE_TARBALL_PATH} ${ALPINE_URL}
    (cd ${CACHE_DIR} && sha256sum -c ${ALPINE_TARBALL}.sha256)
PLAN
        return
    fi

    mkdir -p "${CACHE_DIR}"

    if ! curl -fsSIL "${ALPINE_URL}" >/dev/null 2>&1; then
        die "alpine tarball URL 404s: ${ALPINE_URL}
       Try a different version with --alpine-version <X.Y.Z>.
       Browse available versions at https://dl-cdn.alpinelinux.org/alpine/"
    fi

    curl -fsSL -o "${ALPINE_SHA_PATH}" "${ALPINE_SHA_URL}"

    local need_download=1
    if [[ -f "${ALPINE_TARBALL_PATH}" ]]; then
        if (cd "${CACHE_DIR}" && sha256sum -c "${ALPINE_TARBALL}.sha256") >/dev/null 2>&1; then
            log "alpine tarball cached + sha256 ok (${ALPINE_TARBALL_PATH})"
            need_download=0
        else
            warn "cached alpine tarball sha256 mismatch; re-downloading"
        fi
    fi

    if [[ "${need_download}" -eq 1 ]]; then
        log "downloading alpine tarball to ${ALPINE_TARBALL_PATH}"
        curl -fsSL -o "${ALPINE_TARBALL_PATH}" "${ALPINE_URL}"
        (cd "${CACHE_DIR}" && sha256sum -c "${ALPINE_TARBALL}.sha256") \
            || die "sha256 verification failed for ${ALPINE_TARBALL_PATH}"
        log "sha256 verified"
    fi
}

fetch_alpine

# -----------------------------------------------------------------------------
# Cross-install armhf packages into a staging dir.
#
# This handles the openssh + wpa_supplicant + linux-firmware-rtlwifi
# additions: install them into /tmp/pkg-staging (with --root + --arch armhf),
# stash the downloaded .apk files in /tmp/pkg-cache, then later:
#   - merge the staging tree into the apkovl staging (gives the Pi a tmpfs
#     root that already has wpa_supplicant, sshd, libssl, firmware, etc.)
#   - drop the .apk files into the FAT /cache/ directory (so `apk add` on
#     the Pi can find them later from the local cache)
# -----------------------------------------------------------------------------

PKGS_TO_INSTALL=()
[[ "${SSH_ENABLED}" -eq 1 ]]   && PKGS_TO_INSTALL+=(openssh)
[[ "${WIFI_ENABLED}" -eq 1 ]]  && PKGS_TO_INSTALL+=(wpa_supplicant linux-firmware-rtlwifi)

if [[ "${#PKGS_TO_INSTALL[@]}" -gt 0 ]]; then
    PKG_STAGING="$(mktemp -d -t workplacesim-pkgs.XXXXXX)"
    PKG_CACHE="$(mktemp -d -t workplacesim-pkgcache.XXXXXX)"
    log "cross-installing armhf packages: ${PKGS_TO_INSTALL[*]}"
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        cat <<PLAN
    apk add --root ${PKG_STAGING} --arch armhf --initdb --no-scripts \\
        --allow-untrusted --cache-dir ${PKG_CACHE} \\
        --repository ${ALPINE_REPO_MAIN} \\
        --repository ${ALPINE_REPO_COMMUNITY} \\
        ${PKGS_TO_INSTALL[*]}
PLAN
    else
        apk add --root "${PKG_STAGING}" --arch armhf --initdb --no-scripts \
            --allow-untrusted --cache-dir "${PKG_CACHE}" \
            --repository "${ALPINE_REPO_MAIN}" \
            --repository "${ALPINE_REPO_COMMUNITY}" \
            "${PKGS_TO_INSTALL[@]}" 2>&1 | tail -3
        log "pkg-staging: $(du -sh "${PKG_STAGING}" | cut -f1); cache: $(ls "${PKG_CACHE}"/*.apk 2>/dev/null | wc -l) .apk files"
    fi
fi

# -----------------------------------------------------------------------------
# 1. Assemble the apkovl staging tree
# -----------------------------------------------------------------------------

STAGING_DIR="$(mktemp -d -t workplacesim-staging.XXXXXX)"
log "staging apkovl tree at ${STAGING_DIR}"

install -d -m 0755 "${STAGING_DIR}/etc/apk"
install -d -m 0755 "${STAGING_DIR}/etc/init.d"
install -d -m 0755 "${STAGING_DIR}/etc/local.d"
install -d -m 0755 "${STAGING_DIR}/etc/network"
install -d -m 0755 "${STAGING_DIR}/etc/runlevels/sysinit"
install -d -m 0755 "${STAGING_DIR}/etc/runlevels/boot"
install -d -m 0755 "${STAGING_DIR}/etc/runlevels/default"
install -d -m 0755 "${STAGING_DIR}/usr/local/bin"

# Static overlay files: inittab (getty disabled), local.d hook. We *don't*
# copy hostname or network/interfaces from the overlay tree — those are
# generated below from the relevant CLI flags so a single overlay can
# serve multiple deployments.
cp -a "${OVERLAY_DIR}/etc/inittab" "${STAGING_DIR}/etc/inittab"
install -m 0755 "${OVERLAY_DIR}/etc/local.d/workplacesim.start" \
    "${STAGING_DIR}/etc/local.d/workplacesim.start"

# Hostname (CLI flag, default 'workplacesim').
printf '%s\n' "${HOSTNAME_VAL}" > "${STAGING_DIR}/etc/hostname"

# initd is canonical in deploy/workplacesim.initd.
install -m 0755 "${INITD_FILE}" "${STAGING_DIR}/etc/init.d/workplacesim"

# Cross-built binary.
install -m 0755 "${BIN_ABS}" "${STAGING_DIR}/usr/local/bin/workplacesim"

# Merge cross-installed packages (binaries, libs, firmware) into the
# staging tree. Skip /etc/apk and /lib/apk to keep the Pi's apk db in sync
# with the base tarball; we'll write /etc/apk/world ourselves below.
# Skip /var/cache because /cache on the FAT partition is the authoritative
# cache location.
if [[ -n "${PKG_STAGING}" ]] && [[ "${DRY_RUN}" -ne 1 ]]; then
    log "merging package staging into apkovl"
    (cd "${PKG_STAGING}" && tar cf - \
        --exclude=./etc/apk --exclude=./lib/apk --exclude=./var/cache \
        --exclude=./dev --exclude=./proc --exclude=./sys --exclude=./run --exclude=./tmp \
        . ) | (cd "${STAGING_DIR}" && tar xf - --skip-old-files)
fi

# /etc/apk/repositories: local /apks + our /cache + online repos so future
# `apk add` works both offline (from /cache) and online.
cat > "${STAGING_DIR}/etc/apk/repositories" <<R
/media/mmcblk0p1/apks
/media/mmcblk0p1/cache
${ALPINE_REPO_MAIN}
${ALPINE_REPO_COMMUNITY}
R

# DNS so the Pi can resolve names out of the box. systemd-resolved is not
# used in Alpine; udhcpc rewrites resolv.conf if DHCP gives one, otherwise
# this default survives.
echo "nameserver 1.1.1.1" > "${STAGING_DIR}/etc/resolv.conf"

# /etc/apk/world declares the packages the Pi is expected to have. Listing
# only the things we add on top of alpine-base; the base packages from the
# rpi tarball are unaffected (their installed-db lives in the modloop).
{
    printf 'alpine-base\n'
    [[ "${SSH_ENABLED}" -eq 1 ]] && printf 'openssh\n'
    [[ "${WIFI_ENABLED}" -eq 1 ]] && {
        printf 'wpa_supplicant\n'
        printf 'linux-firmware-rtlwifi\n'
    }
} > "${STAGING_DIR}/etc/apk/world"

# /etc/network/interfaces. eth0 is either DHCP (default) or static
# (--static-ip). wlan0 only present if wifi was enabled.
{
    cat <<I
auto lo
iface lo inet loopback

I
    if [[ -n "${STATIC_IP}" ]]; then
        # Convert CIDR (e.g. 192.168.2.10/24) to address + netmask. /24 is
        # the only common case worth special-casing inline; for other masks
        # users can edit on the Pi or set --gateway separately.
        ipaddr="${STATIC_IP%/*}"
        prefix="${STATIC_IP#*/}"
        # tiny prefix-to-netmask LUT, covering the common cases
        case "${prefix}" in
            24) netmask="255.255.255.0" ;;
            16) netmask="255.255.0.0" ;;
            8)  netmask="255.0.0.0" ;;
            *)  netmask="" ;;
        esac
        cat <<I
auto eth0
iface eth0 inet static
    address ${ipaddr}
$(if [[ -n "${netmask}" ]]; then printf '    netmask %s\n' "${netmask}"; fi)
$(if [[ -n "${GATEWAY}" ]]; then printf '    gateway %s\n' "${GATEWAY}"; fi)
I
    else
        cat <<I
auto eth0
iface eth0 inet dhcp
I
    fi
    if [[ "${WIFI_ENABLED}" -eq 1 ]]; then
        cat <<I

auto wlan0
iface wlan0 inet dhcp
    pre-up /sbin/wpa_supplicant -B -i wlan0 -D nl80211,wext -c /etc/wpa_supplicant/wpa_supplicant.conf
    post-down /bin/killall -q wpa_supplicant || true
I
    fi
} > "${STAGING_DIR}/etc/network/interfaces"

# Wifi creds (only if wifi enabled). chmod 600 because the PSK is in
# plaintext.
if [[ "${WIFI_ENABLED}" -eq 1 ]]; then
    install -d -m 0755 "${STAGING_DIR}/etc/wpa_supplicant"
    cat > "${STAGING_DIR}/etc/wpa_supplicant/wpa_supplicant.conf" <<WPA
ctrl_interface=/var/run/wpa_supplicant
update_config=1

network={
    ssid="${WIFI_SSID}"
    psk="${WIFI_PSK}"
    key_mgmt=WPA-PSK
}
WPA
    chmod 600 "${STAGING_DIR}/etc/wpa_supplicant/wpa_supplicant.conf"
fi

# SSH: pre-generate host keys (so they persist across boots in this single-
# Pi setup), write sshd_config, install the user's pubkey.
if [[ "${SSH_ENABLED}" -eq 1 ]] && [[ "${DRY_RUN}" -ne 1 ]]; then
    log "generating sshd host keys + installing pubkey"
    install -d -m 0755 "${STAGING_DIR}/etc/ssh"
    ssh-keygen -t ed25519        -N '' -f "${STAGING_DIR}/etc/ssh/ssh_host_ed25519_key" -q
    ssh-keygen -t rsa -b 2048    -N '' -f "${STAGING_DIR}/etc/ssh/ssh_host_rsa_key"     -q
    ssh-keygen -t ecdsa          -N '' -f "${STAGING_DIR}/etc/ssh/ssh_host_ecdsa_key"   -q
    chmod 600 "${STAGING_DIR}/etc/ssh/"ssh_host_*_key
    chmod 644 "${STAGING_DIR}/etc/ssh/"ssh_host_*_key.pub
    cat > "${STAGING_DIR}/etc/ssh/sshd_config" <<SSH
Port 22
PermitRootLogin yes
PubkeyAuthentication yes
PasswordAuthentication yes
PermitEmptyPasswords no
SSH
    install -d -m 0700 "${STAGING_DIR}/root/.ssh"
    cp "${SSH_PUBKEY}" "${STAGING_DIR}/root/.ssh/authorized_keys"
    chmod 600 "${STAGING_DIR}/root/.ssh/authorized_keys"
fi

# Runlevel symlinks. Our apkovl REPLACES the base /etc/runlevels/* dirs
# (because tar extraction merges directory contents — empty dirs in the
# apkovl effectively wipe the base symlinks). So we must declare every
# service we want started, including the standard Alpine ones we depend on
# (modloop is the load-bearing one: without it /lib/modules is empty and
# no kernel module loads).
for s in devfs dmesg hwdrivers mdev modloop sysfs; do
    ln -sf "/etc/init.d/$s" "${STAGING_DIR}/etc/runlevels/sysinit/$s"
done
for s in bootmisc hostname hwclock modules swclock sysctl syslog; do
    ln -sf "/etc/init.d/$s" "${STAGING_DIR}/etc/runlevels/boot/$s"
done
for s in local networking workplacesim; do
    ln -sf "/etc/init.d/$s" "${STAGING_DIR}/etc/runlevels/default/$s"
done
[[ "${SSH_ENABLED}" -eq 1 ]] && \
    ln -sf "/etc/init.d/sshd" "${STAGING_DIR}/etc/runlevels/default/sshd"

# -----------------------------------------------------------------------------
# 2. Tar the staging dir into workplacesim.apkovl.tar.gz
# -----------------------------------------------------------------------------

APKOVL_TARBALL="${STAGING_DIR}/../workplacesim.apkovl.tar.gz"
log "tar apkovl -> ${APKOVL_TARBALL}"
if [[ "${DRY_RUN}" -eq 1 ]]; then
    cat <<PLAN
    tar --owner=root --group=root --numeric-owner \\
        -C ${STAGING_DIR} -czf ${APKOVL_TARBALL} .
PLAN
else
    tar --owner=root --group=root --numeric-owner \
        -C "${STAGING_DIR}" -czf "${APKOVL_TARBALL}" .
    log "apkovl size: $(du -sh "${APKOVL_TARBALL}" | cut -f1)"
fi

# -----------------------------------------------------------------------------
# 3. Build the SD-card image (partition table + FAT32 + Alpine tarball)
# -----------------------------------------------------------------------------

log "output image: ${OUTPUT} (size: $((IMAGE_SIZE_BYTES / 1024 / 1024)) MiB)"
if [[ "${DRY_RUN}" -eq 1 ]]; then
    cat <<PLAN
    truncate -s ${IMAGE_SIZE_BYTES} ${OUTPUT}
    LOOP=\$(losetup -fP --show ${OUTPUT})
    parted -s \$LOOP mklabel msdos \\
        mkpart primary fat32 1MiB 100% \\
        set 1 boot on \\
        set 1 lba on
    partx -a \$LOOP                         # force kernel to register partition node
    mkfs.vfat -F32 -n WORKPLACE \${LOOP}p1
    MOUNT=\$(mktemp -d)
    mount \${LOOP}p1 \$MOUNT
    tar -xf ${ALPINE_TARBALL_PATH} -C \$MOUNT
    cp ${APKOVL_TARBALL} \$MOUNT/workplacesim.apkovl.tar.gz
PLAN
    if [[ -n "${PKG_CACHE}" ]]; then
        cat <<PLAN
    mkdir -p \$MOUNT/cache && cp ${PKG_CACHE}/*.apk \$MOUNT/cache/
PLAN
    fi
    cat <<PLAN
    sync; umount \$MOUNT; losetup -d \$LOOP
PLAN
    log "dry-run complete. no filesystem side effects."
    exit 0
fi

if [[ -e "${OUTPUT}" ]]; then
    warn "overwriting existing ${OUTPUT}"
    rm -f -- "${OUTPUT}"
fi

log "truncate -s ${IMAGE_SIZE_BYTES} ${OUTPUT}"
truncate -s "${IMAGE_SIZE_BYTES}" "${OUTPUT}"

log "losetup -fP --show ${OUTPUT}"
LOOP_DEV="$(losetup -fP --show "${OUTPUT}")"
log "loop device: ${LOOP_DEV}"

log "parted: single fat32 boot partition"
parted -s "${LOOP_DEV}" \
    mklabel msdos \
    mkpart primary fat32 1MiB 100% \
    set 1 boot on \
    set 1 lba on

# Force the kernel to notice the new partition table. `losetup -P` relies
# on udev to create /dev/loopNpX nodes; inside a minimal Docker container
# there is no running udev, so we use partx's BLKPG ioctl directly.
# partprobe is belt-and-suspenders on hosts that do run udev.
partx -a "${LOOP_DEV}" 2>/dev/null || true
if command -v partprobe >/dev/null 2>&1; then
    partprobe "${LOOP_DEV}" 2>/dev/null || true
fi

PART_DEV="${LOOP_DEV}p1"
if [[ ! -b "${PART_DEV}" ]]; then
    # Some container environments don't auto-create the partition node even
    # after BLKPG succeeds. Create it by hand from the major/minor exposed
    # in sysfs.
    part_name="$(basename "${LOOP_DEV}")p1"
    if [[ -f "/sys/class/block/${part_name}/dev" ]]; then
        IFS=: read -r major minor < "/sys/class/block/${part_name}/dev"
        mknod "${PART_DEV}" b "${major}" "${minor}" || true
    fi
fi
[[ -b "${PART_DEV}" ]] || die "partition device ${PART_DEV} did not appear after parted"

log "mkfs.vfat -F32 -n WORKPLACE ${PART_DEV}"
mkfs.vfat -F32 -n WORKPLACE "${PART_DEV}" >/dev/null

MOUNT_DIR="$(mktemp -d -t workplacesim-mkimage.XXXXXX)"
log "mount ${PART_DEV} -> ${MOUNT_DIR}"
mount "${PART_DEV}" "${MOUNT_DIR}"

log "extract alpine rpi tarball into FAT (firmware + kernel + modloop)"
tar -xf "${ALPINE_TARBALL_PATH}" -C "${MOUNT_DIR}"

log "drop workplacesim.apkovl.tar.gz at FAT root"
cp "${APKOVL_TARBALL}" "${MOUNT_DIR}/workplacesim.apkovl.tar.gz"

if [[ -n "${PKG_CACHE}" ]] && ls "${PKG_CACHE}"/*.apk >/dev/null 2>&1; then
    log "drop $(ls "${PKG_CACHE}"/*.apk | wc -l) cached .apk files into FAT /cache/"
    install -d -m 0755 "${MOUNT_DIR}/cache"
    cp "${PKG_CACHE}"/*.apk "${MOUNT_DIR}/cache/"
fi

log "sync + umount"
sync
umount "${MOUNT_DIR}"
rmdir "${MOUNT_DIR}"
MOUNT_DIR=""

log "detach loop device ${LOOP_DEV}"
losetup -d "${LOOP_DEV}"
LOOP_DEV=""

log "cleanup staging trees"
rm -rf -- "${STAGING_DIR}"; STAGING_DIR=""
[[ -n "${PKG_STAGING}" ]] && rm -rf -- "${PKG_STAGING}" && PKG_STAGING=""
[[ -n "${PKG_CACHE}"   ]] && rm -rf -- "${PKG_CACHE}"   && PKG_CACHE=""

log "done"
printf '\nimage: %s\n' "${OUTPUT}"
du -sh -- "${OUTPUT}" | awk '{print "size: " $1}'
cat <<NEXT

Flash to an SD card (replace /dev/sdX with your card, NOT your system disk):

  sudo dd if=${OUTPUT} of=/dev/sdX bs=4M status=progress conv=fsync

Boot the Pi. On first boot:
  - Alpine extracts workplacesim.apkovl.tar.gz onto the tmpfs root
    (binary, initd, hostname, network, runlevels, plus optional
    sshd / wpa_supplicant / firmware that were baked in).
  - OpenRC starts sysinit -> boot -> default runlevels. modloop mounts
    the kernel module squashfs, networking brings eth0 up, sshd starts
    if enabled, workplacesim renders to /dev/fb0.
NEXT
[[ "${SSH_ENABLED}" -eq 1 ]] && printf '\n  ssh root@%s.local        # passwordless via baked-in pubkey\n' "${HOSTNAME_VAL}"
[[ -n "${STATIC_IP}" ]]      && printf '  ssh root@%s   # static IP from --static-ip\n' "${STATIC_IP%/*}"
printf '  curl http://%s.local:4317/                 # workplacesim HTTP/SSE\n' "${HOSTNAME_VAL}"
