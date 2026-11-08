#!/usr/bin/env bash
# Build CamCraft's containerised targets and copy the packages to dist/.
# Provenance: docker/README.md; `docker build --output` (BuildKit) docs.
#   docker/build.sh            # linux (tar.gz, deb, rpm, AppImage)
#   docker/build.sh windows    # windows-gnu portable zip (unofficial)
#   docker/build.sh all
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
command -v docker >/dev/null || { echo "docker/build.sh: docker is not on PATH" >&2; exit 1; }
targets=("${@:-linux}")
[[ ${targets[0]} == all ]] && targets=(linux windows)
for t in "${targets[@]}"; do
    case $t in
        linux) file=docker/linux.Dockerfile ;;
        windows) file=docker/windows-gnu.Dockerfile ;;
        *) echo "docker/build.sh: unknown target '$t' (linux, windows, all)" >&2; exit 2 ;;
    esac
    DOCKER_BUILDKIT=1 docker build -f "$file" --target packages \
        --output "type=local,dest=dist" .
done
ls -l dist
