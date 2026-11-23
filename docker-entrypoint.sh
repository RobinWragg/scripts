#!/bin/sh

set -u

channels_root="${VID_CHANNELS_ROOT_PATH:-/work}"
ytdlp_path="${channels_root}/yt-dlp"
mkdir -p "$channels_root"
export PATH="${channels_root}:$PATH"

case "$(uname -m)" in
    aarch64|arm64)
        ytdlp_asset="yt-dlp_linux_aarch64"
        ;;
    armv7l)
        ytdlp_asset="yt-dlp_linux_armv7l"
        ;;
    x86_64|amd64)
        ytdlp_asset="yt-dlp_linux"
        ;;
    *)
        ytdlp_asset=""
        ;;
esac

if [ ! -x "$ytdlp_path" ] && [ -n "$ytdlp_asset" ]; then
    echo "Bootstrapping yt-dlp in ${ytdlp_path}"
    ytdlp_tmp="${ytdlp_path}.tmp"
    if curl -fL --retry 3 \
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/${ytdlp_asset}" \
        -o "$ytdlp_tmp"; then
        chmod 0755 "$ytdlp_tmp"
        mv "$ytdlp_tmp" "$ytdlp_path"
    else
        rm -f "$ytdlp_tmp"
        echo "Warning: could not download yt-dlp; continuing without it" >&2
    fi
elif [ ! -x "$ytdlp_path" ]; then
    echo "Warning: unsupported architecture $(uname -m); continuing without yt-dlp" >&2
fi

if [ -x "$ytdlp_path" ]; then
    echo "Checking for yt-dlp updates"
    if "$ytdlp_path" --update; then
        echo "yt-dlp update check complete"
    else
        echo "Warning: yt-dlp update check failed; using the cached executable" >&2
    fi
fi

exec /usr/local/bin/vid "$@" > "${channels_root}/log.txt" 2>&1
