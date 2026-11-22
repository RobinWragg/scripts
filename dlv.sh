#!/usr/bin/env bash

set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 <video-url>" >&2
  exit 1
fi

url="$1"
downloads_dir="$HOME/Downloads"
target_dir="/Volumes/main/server/docker/vid_storage/videos"

downloaded_file="$(yt-dlp --no-playlist -P "$downloads_dir" -S +height:800 --print after_move:filepath "$url")"

echo "Downloaded: $downloaded_file"

if mv "$downloaded_file" "$target_dir/"; then
  echo "Moved to: $target_dir"
else
  echo "Could not move file to $target_dir. The file remains at: $downloaded_file"
fi