#!/bin/bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
profile=release
build=1
install=0
install_app="/Applications/Goose herdr GPUI.app"
old_install_app=/Applications/goose-herdr-gpui.app
output="$root/dist"
while (( $# )); do
  case "$1" in
    --debug) profile=debug ;;
    --no-build) build=0 ;;
    --install) install=1 ;;
    --output) [[ $# -ge 2 && -n $2 && $2 != --* ]] || { echo "--output requires a directory" >&2; exit 2; }; output=$2; shift ;;
    *) echo 'usage: scripts/build-app.sh [--debug] [--no-build] [--install] [--output directory]' >&2; exit 2 ;;
  esac
  shift
done
[[ $(uname -s) == Darwin ]] || { echo 'macOS is required' >&2; exit 1; }
source_root=${HERDR_SOURCE_DIR:-"$root/../goose-herdr"}
[[ -f "$source_root/scripts/package-usage-helper.sh" ]] || { echo 'Set HERDR_SOURCE_DIR to the existing Goose Herdr source tree' >&2; exit 1; }
if (( build )); then
  # 保留调用方 flags，仅追加：去掉二进制里的本机绝对路径
  remap_flags=("--remap-path-prefix=${root}=/build")
  if [[ -n ${HOME:-} ]]; then remap_flags+=("--remap-path-prefix=${HOME}=/build"); fi
  if [[ -n ${CARGO_ENCODED_RUSTFLAGS:-} ]]; then
    encoded=$CARGO_ENCODED_RUSTFLAGS
    for flag in "${remap_flags[@]}"; do encoded+=$'\x1f'"$flag"; done
    export CARGO_ENCODED_RUSTFLAGS=$encoded
  else
    export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }${remap_flags[*]}"
  fi
  if [[ $profile == release ]]; then cargo build --locked --release; else cargo build --locked; fi
fi
target_dir=${CARGO_TARGET_DIR:-"$root/target"}
if [[ -n ${CARGO_BUILD_TARGET:-} ]]; then target_dir="$target_dir/$CARGO_BUILD_TARGET"; fi
binary="$target_dir/$profile/goose-herdr-gpui"
[[ -x "$binary" ]] || { echo "Missing executable: $binary" >&2; exit 1; }
mkdir -p "$output"
output=$(cd "$output" && pwd)
app="$output/Goose herdr GPUI.app"
if [[ -e "$app" ]]; then
  if (( install )); then
    rm -rf "$app"
  else
    echo "Refusing to overwrite $app; move the previous bundle first" >&2
    exit 1
  fi
fi
staging=$(mktemp -d "$output/.app-build.XXXXXX")
trap 'rm -rf "$staging"' EXIT
bundle="$staging/Goose herdr GPUI.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$binary" "$bundle/Contents/MacOS/goose-herdr-gpui"
cp "$root/resources/Info.plist" "$bundle/Contents/Info.plist"
icon_src="$source_root/Sources/GooseAgent/AppIcon.icon"
[[ -f "$icon_src/icon.json" ]] || { echo "Missing app icon: $icon_src" >&2; exit 1; }
xcrun actool "$icon_src" --compile "$bundle/Contents/Resources" \
  --platform macosx --minimum-deployment-target 12.0 --app-icon AppIcon \
  --output-partial-info-plist "$staging/icon-info.plist" --output-format human-readable-text
[[ -s "$bundle/Contents/Resources/Assets.car" && -s "$bundle/Contents/Resources/AppIcon.icns" ]] || {
  echo 'Icon compilation did not produce Assets.car and AppIcon.icns' >&2; exit 1;
}
mkdir -p "$bundle/Contents/Resources/icons"
cp "$root/resources/icons/LICENSES" "$root/resources/icons/SOURCES.md" "$bundle/Contents/Resources/icons/"
mkdir -p "$bundle/Contents/Resources/licenses/zed-terminal"
cp "$root/vendor/zed-terminal/LICENSE-GPL" "$root/vendor/zed-terminal/LICENSE-APACHE-ALACRITTY" "$root/vendor/zed-terminal/SOURCES.md" "$bundle/Contents/Resources/licenses/zed-terminal/"
cp -R "$root/resources/licenses/gpui-component" "$bundle/Contents/Resources/licenses/"
for folder in AgentIcons OSIcons SpaceIcons AppIcon; do
  if [[ -d $source_root/Resources/$folder ]]; then
    cp -R "$source_root/Resources/$folder" "$bundle/Contents/Resources/icons/"
  fi
done
arch=$(lipo -archs "$binary")
case "$arch" in arm64|x86_64) ;; *) echo 'Build one macOS architecture at a time' >&2; exit 1 ;; esac
bash "$source_root/scripts/package-usage-helper.sh" "$bundle/Contents/Resources/UsageHelper" "$arch"
# ponytail: Tailcat is statically linked today; fail closed if a future build adds external dylibs.
if otool -L "$binary" | tail -n +2 | grep -Ev '^[[:space:]]+(/System/Library/|/usr/lib/)' | grep -q .; then
  echo 'External dylib dependency detected; bundle and rewrite its install name before packaging' >&2
  exit 1
fi
plutil -lint "$bundle/Contents/Info.plist"
# 裁剪符号：必须在 codesign 之前（strip 会使内嵌签名失效）
strip -x "$bundle/Contents/MacOS/goose-herdr-gpui"
node_binary="$bundle/Contents/Resources/UsageHelper/runtime/$arch/node"
[[ -x $node_binary ]] || { echo "Missing node runtime: $node_binary" >&2; exit 1; }
strip -x "$node_binary"
# strip 会使 node 内嵌签名失效；必须带 hardened runtime + allow-jit 再签，否则内核 SIGKILL
node_entitlements="$source_root/UsageHelper/runtime.entitlements"
if [[ -f $node_entitlements ]]; then
  codesign --force --sign - --options runtime --entitlements "$node_entitlements" "$node_binary"
else
  codesign --force --sign - "$node_binary"
fi
# 先签内部 Mach-O，再签 bundle（不用 --deep，避免盖掉 node 的 JIT 授权）
codesign --force --sign - "$bundle"
[[ ! -e $app ]] || rm -rf "$app"
mv "$bundle" "$app"
if (( install )); then
  running=0
  if pgrep -xq goose-herdr-gpui || pgrep -qf "$install_app/Contents/MacOS/goose-herdr-gpui" || pgrep -qf "$old_install_app/Contents/MacOS/goose-herdr-gpui"; then
    running=1
  fi
  if (( running )); then
    osascript -e 'tell application id "dev.eachann.goose-herdr-gpui" to quit' >/dev/null 2>&1 || true
    for _ in {1..20}; do
      pgrep -xq goose-herdr-gpui || pgrep -qf "$install_app/Contents/MacOS/goose-herdr-gpui" || pgrep -qf "$old_install_app/Contents/MacOS/goose-herdr-gpui" || break
      sleep 0.25
    done
    if pgrep -xq goose-herdr-gpui || pgrep -qf "$install_app/Contents/MacOS/goose-herdr-gpui" || pgrep -qf "$old_install_app/Contents/MacOS/goose-herdr-gpui"; then
      pkill -x goose-herdr-gpui || true
      sleep 0.2
    fi
  fi
  rm -rf "$install_app" "$old_install_app"
  ditto "$app" "$install_app"
  if (( running )); then
    open "$install_app"
  fi
  printf '%s\n' "$app" "$install_app"
else
  printf '%s\n' "$app"
fi
