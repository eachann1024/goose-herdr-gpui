#!/bin/sh
set -eu
: "${OUT_DIR:?OUT_DIR must be set by Cargo}"
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
FRAMEWORK_DIR=${HERDR_TAILCAT_FRAMEWORK_DIR:-"$ROOT/../goose-herdr/Packages/HerdrTailcat/Artifacts/Tailcat.xcframework/macos-arm64_x86_64"}
if [ ! -f "$FRAMEWORK_DIR/Tailcat.framework/Tailcat" ]; then
    echo 'Tailcat.framework missing; set HERDR_TAILCAT_FRAMEWORK_DIR to its containing directory' >&2
    exit 1
fi
ARCH=${CARGO_CFG_TARGET_ARCH:-$(uname -m)}
case "$ARCH" in aarch64) ARCH=arm64 ;; esac
xcrun clang -arch "$ARCH" -fobjc-arc -fmodules -F "$FRAMEWORK_DIR" -c "$ROOT/native/TailcatBridge.m" -o "$OUT_DIR/TailcatBridge.o"
xcrun ar rcs "$OUT_DIR/libherdr_tailcat_bridge.a" "$OUT_DIR/TailcatBridge.o"
printf '%s\n' "cargo:rerun-if-changed=$FRAMEWORK_DIR/Tailcat.framework/Tailcat" "cargo:rustc-link-search=native=$OUT_DIR" "cargo:rustc-link-search=framework=$FRAMEWORK_DIR" 'cargo:rustc-link-lib=static=herdr_tailcat_bridge' 'cargo:rustc-link-lib=framework=Tailcat' 'cargo:rustc-link-lib=framework=Foundation' 'cargo:rustc-link-lib=framework=Security' 'cargo:rustc-link-lib=framework=CoreFoundation' 'cargo:rustc-link-lib=resolv'
