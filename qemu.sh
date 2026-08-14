#!/usr/bin/env bash
# Tacit run command: boot the AArch64 image under QEMU virt with no guest
# Linux/macOS.  HVF is used on this Apple Silicon Mac when available; TCG is
# the documented fallback.
set -euo pipefail

cd "$(dirname "$0")"

IMG=build/tacit.elf
RAM="${RAM:-1G}"
DISPLAY="${DISPLAY_MODE:-cocoa}"

if [ ! -f "$IMG" ]; then
  echo "qemu: no image at $IMG — run ./build.sh first" >&2
  exit 1
fi

if qemu-system-aarch64 -accel help 2>/dev/null | grep -q hvf; then
  ACCEL=hvf
  CPU=host
else
  echo "qemu: HVF not available — falling back to TCG (software emulation)" >&2
  ACCEL=tcg
  CPU=max
fi

echo "qemu: booting $IMG on aarch64 virt (accel=$ACCEL)"

exec qemu-system-aarch64 \
  -M virt \
  -cpu "$CPU" \
  -accel "$ACCEL" \
  -m "$RAM" \
  -smp 1 \
  -device ramfb \
  -display "$DISPLAY" \
  -serial mon:stdio \
  -kernel "$IMG"
