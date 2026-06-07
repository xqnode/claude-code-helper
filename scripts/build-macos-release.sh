#!/usr/bin/env bash
# 在 macOS 上构建 Claude Code Helper.app 与 DMG（Universal：Apple Silicon + Intel）。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"
VERSION="$(grep '^version' "$ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')"
APP_NAME="Claude Code Helper"
APP_BUNDLE="$DIST/${APP_NAME}.app"
DMG_PATH="$DIST/ClaudeCodeHelper-${VERSION}-macos.dmg"
UNIVERSAL_BINARY="$ROOT/target/universal/claude-code-helper"
ICON_PNG="$ROOT/assets/claude-code-helper.png"
INFO_PLIST="$ROOT/installer/macos/Info.plist"
README="$ROOT/installer/USAGE-zh-CN.txt"

echo "Claude Code Helper v${VERSION} (macOS)"

cd "$ROOT"

HOST_ARCH="$(uname -m)"
echo "Host: ${HOST_ARCH} — $(sw_vers -productName 2>/dev/null || echo macOS) $(sw_vers -productVersion 2>/dev/null || true)"
echo ""

build_release_binary() {
    if [[ "${NATIVE_ONLY:-}" == "1" ]]; then
        echo "Fast build: native ${HOST_ARCH} only"
        cargo build --release
        mkdir -p "$ROOT/target/universal"
        cp "$ROOT/target/release/claude-code-helper" "$UNIVERSAL_BINARY"
        return
    fi

    echo "Universal build: arm64 + x86_64"
    rustup target add aarch64-apple-darwin x86_64-apple-darwin

    echo "  → cargo build --release --target aarch64-apple-darwin"
    cargo build --release --target aarch64-apple-darwin

    echo "  → cargo build --release --target x86_64-apple-darwin"
    cargo build --release --target x86_64-apple-darwin

    mkdir -p "$ROOT/target/universal"
    lipo -create \
        "$ROOT/target/aarch64-apple-darwin/release/claude-code-helper" \
        "$ROOT/target/x86_64-apple-darwin/release/claude-code-helper" \
        -output "$UNIVERSAL_BINARY"
}

build_release_binary
BINARY="$UNIVERSAL_BINARY"
chmod +x "$BINARY"
echo "  OK  $(lipo -info "$BINARY")"

if [[ ! -f "$BINARY" ]]; then
    echo "Missing binary: $BINARY" >&2
    exit 1
fi

if [[ ! -f "$ICON_PNG" ]]; then
    echo "Missing icon PNG: $ICON_PNG" >&2
    echo "Run: cargo build --release (build.rs generates PNG from icon_render.rs)" >&2
    exit 1
fi

install_app_icon() {
    local app_bundle="$1"
    local iconset="$DIST/AppIcon.iconset"
    local icns_path="$app_bundle/Contents/Resources/AppIcon.icns"

    rm -rf "$iconset"
    mkdir -p "$iconset"
    for size in 16 32 128 256 512; do
        sips -z "$size" "$size" "$ICON_PNG" \
            --out "$iconset/icon_${size}x${size}.png" >/dev/null
        local double=$((size * 2))
        sips -z "$double" "$double" "$ICON_PNG" \
            --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
    done
    iconutil -c icns "$iconset" -o "$icns_path"
    rm -rf "$iconset"
    echo "  OK  AppIcon.icns"
}

rm -rf "$APP_BUNDLE"
mkdir -p "$APP_BUNDLE/Contents/MacOS" "$APP_BUNDLE/Contents/Resources"

cp "$BINARY" "$APP_BUNDLE/Contents/MacOS/claude-code-helper"
chmod +x "$APP_BUNDLE/Contents/MacOS/claude-code-helper"

cp "$INFO_PLIST" "$APP_BUNDLE/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString ${VERSION}" "$APP_BUNDLE/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion ${VERSION}" "$APP_BUNDLE/Contents/Info.plist"

install_app_icon "$APP_BUNDLE"

if [[ -f "$README" ]]; then
    cp "$README" "$APP_BUNDLE/Contents/Resources/USAGE-zh-CN.txt"
fi

if codesign --force --deep --sign - "$APP_BUNDLE" 2>/dev/null; then
    echo "  OK  ad-hoc codesign"
else
    echo "  WARN  codesign skipped (non-fatal)"
fi

mkdir -p "$DIST"
STAGING="$DIST/dmg-staging"
rm -rf "$STAGING"
mkdir -p "$STAGING"
cp -R "$APP_BUNDLE" "$STAGING/"
ln -s /Applications "$STAGING/Applications"
if [[ -f "$README" ]]; then
    cp "$README" "$STAGING/USAGE-zh-CN.txt"
fi

cp "$APP_BUNDLE/Contents/Resources/AppIcon.icns" "$STAGING/.VolumeIcon.icns"
if command -v SetFile >/dev/null 2>&1; then
    SetFile -a C "$STAGING"
fi

rm -f "$DMG_PATH"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGING" -ov -format UDZO "$DMG_PATH"
rm -rf "$STAGING"

SIZE_MB="$(du -m "$DMG_PATH" | awk '{print $1}')"
echo ""
echo "Done."
echo "  App: $APP_BUNDLE"
echo "  DMG: $DMG_PATH (${SIZE_MB} MB)"
echo ""
echo "Note: 公开发布前需 codesign + notarize，否则 Gatekeeper 可能拦截。"
echo "Tip: 本机快速测试: NATIVE_ONLY=1 ./scripts/build-macos-release.sh"
