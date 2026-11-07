#!/usr/bin/env bash
# Bundle the Linux release binaries and adjacent data in one AppImage.
# Provenance: linuxdeploy's AppDir and output-plugin documentation.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
if [[ $(uname -s) != Linux || $(uname -m) != x86_64 ]]; then
    echo "package-appimage: Linux x86_64 is required" >&2
    exit 1
fi
linuxdeploy=${LINUXDEPLOY:-}
if [[ -z $linuxdeploy || ! -x $linuxdeploy ]]; then
    echo "package-appimage: set LINUXDEPLOY to an executable linuxdeploy AppImage" >&2
    exit 1
fi
if ! command -v convert >/dev/null 2>&1; then
    echo "package-appimage: ImageMagick convert is required for the 256x256 icon" >&2
    exit 1
fi
bin_dir=${CAMCRAFT_BIN_DIR:-target/release}
for binary in camcraft-ui camcraft-cli post; do
    if [[ ! -x $bin_dir/$binary ]]; then
        echo "package-appimage: missing executable $bin_dir/$binary" >&2
        exit 1
    fi
done
version=$("$bin_dir/post" --version | awk '{print $2}')
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "package-appimage: unexpected post version '$version'" >&2
    exit 1
fi
dist=${CAMCRAFT_DIST_DIR:-dist}
mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
work=$(mktemp -d "$dist/.package-appimage.XXXXXX")
trap 'rm -rf "$work"' EXIT
appdir="$work/CamCraft.AppDir"
mkdir -p "$appdir/usr/bin/assets"
for asset in posts machines ui scripts themes; do
    cp -a "assets/$asset" "$appdir/usr/bin/assets/$asset"
done
cat > "$work/camcraft.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=CamCraft
Comment=Computer-aided manufacturing
Exec=camcraft-ui
Icon=camcraft
Terminal=false
Categories=Graphics;Engineering;
EOF
convert assets/ui/cube.png -resize 256x256 "$work/camcraft.png"
APPIMAGE_EXTRACT_AND_RUN=1 \
LDAI_NO_APPSTREAM=1 \
LDAI_OUTPUT="$dist/camcraft-$version-linux-x86_64.AppImage" \
LINUXDEPLOY_OUTPUT_VERSION="$version" \
    "$linuxdeploy" --appdir "$appdir" \
    --executable "$bin_dir/camcraft-ui" \
    --executable "$bin_dir/camcraft-cli" \
    --executable "$bin_dir/post" \
    --desktop-file "$work/camcraft.desktop" \
    --icon-file "$work/camcraft.png" \
    --output appimage
test -s "$dist/camcraft-$version-linux-x86_64.AppImage"
echo "AppImage: $dist/camcraft-$version-linux-x86_64.AppImage"
