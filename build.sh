#!/usr/bin/env bash
# Tacit build: host compiler (Uiua -> UIR), then the freestanding AArch64
# guest image.  Documented build command for the first QEMU milestone.
set -euo pipefail

cd "$(dirname "$0")"

ARCH="${ARCH:-aarch64}"
POLICY="${POLICY:-policy.ua}"

# Law 1 / first milestone: the only supported boot architecture is AArch64.
case "$ARCH" in
  aarch64|arm64) ;;
  *)
    echo "build: unsupported boot architecture '$ARCH'" >&2
    echo "build: the first milestone target is QEMU aarch64 virt (AArch64); x86_64 is not a supported target" >&2
    exit 1
    ;;
esac

# The documented host is an Apple Silicon Mac; the host compiler runs natively.
if [ "$(uname -m)" != "arm64" ]; then
  echo "build: warning: documented host is an Apple Silicon Mac (arm64); continuing" >&2
fi

echo "== Tacit build =="

echo "-- host compiler"
cargo build --release -p hostc

HC=target/release/hostc
EMB=crates/tacit/embedded
mkdir -p "$EMB"

echo "-- compiling Uiua subset to UIR"
"$HC" --no-fuse -o "$EMB/tiny.uir"     uiua/tiny.ua
"$HC" --no-fuse -o "$EMB/agent.uir"    uiua/agent.ua
"$HC" --no-fuse -o "$EMB/agent-sort.uir" uiua/agent-sort.ua
"$HC" --no-fuse -o "$EMB/agent-pick.uir" uiua/agent-pick.ua
"$HC" --no-fuse -o "$EMB/plan.uir"     uiua/plan.ua
"$HC" --no-fuse -o "$EMB/subset.uir"   uiua/subset.ua
"$HC" --no-fuse -o "$EMB/machine.uir"  uiua/machine.ua
"$HC" --no-fuse -o "$EMB/graph.uir"    uiua/graph.ua
"$HC" --no-fuse -o "$EMB/provenance.uir" uiua/provenance.ua
"$HC" --no-fuse -o "$EMB/objects.uir"  uiua/objects.ua
"$HC" --no-fuse -o "$EMB/replay.uir"   uiua/replay.ua
"$HC" --no-fuse -o "$EMB/bench-send.uir" uiua/bench-send.ua
"$HC" --no-fuse -o "$EMB/policy.uir"   "uiua/$POLICY"
"$HC" --fuse    -o "$EMB/bench-fused.uir"   uiua/bench-fusion.ua
"$HC" --no-fuse -o "$EMB/bench-unfused.uir" uiua/bench-fusion.ua
"$HC" --no-fuse -o "$EMB/bench-matmul.uir"  uiua/bench-matmul.ua

echo "-- guest image (AArch64, no_std microkernel)"
cargo build --release --target aarch64-unknown-none -p tacit

mkdir -p build
cp target/aarch64-unknown-none/release/tacit build/tacit.elf

# Raw flat image (optional; the ELF is the documented artifact).
OBJCOPY="$(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin/llvm-objcopy"
if [ -x "$OBJCOPY" ]; then
  "$OBJCOPY" -O binary target/aarch64-unknown-none/release/tacit build/tacit.img
fi

echo "-- image contract check (no POSIX/Metal/network service symbols, no file paths)"
# The guest now contains the compiler (for the Uiua shell), whose rejection
# vocabulary legitimately includes the words `listen`, `cuda`, `coreml`, and
# so on as error text.  The invariant is about *service machinery*, so the
# symbol table is what we scan; only path literals are still checked in raw
# strings because nothing should ever embed a POSIX path.
OBJTOOLS="$(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin"
if [ -x "$OBJTOOLS/llvm-nm" ]; then
  if "$OBJTOOLS/llvm-nm" build/tacit.elf 2>/dev/null | grep -qiE "listen|accept|posix|fork|mtlcommand|mtldevice|coreml|cuda"; then
    echo "build: image contains a forbidden kernel-service symbol (POSIX/Metal/network path)" >&2
    exit 1
  fi
fi
if strings build/tacit.elf 2>/dev/null | grep -qiE "/etc/|/usr/|/bin/"; then
  echo "build: image contains a POSIX file path" >&2
  exit 1
fi

echo "build: ok"
echo "build: image -> build/tacit.elf (also build/tacit.img)"
