#!/bin/sh

set -eu

if [ "$(uname -s)" = "Darwin" ]; then
	osascript -e 'tell application id "com.robinwragg.plex-menu" to quit' >/dev/null 2>&1 || true

	for _ in 1 2 3 4 5 6 7 8 9 10; do
		pgrep -x plex_menu >/dev/null 2>&1 || break
		sleep 0.1
	done

	for pid in $(pgrep -x plex_menu 2>/dev/null); do
		kill "$pid" >/dev/null 2>&1 || true
	done

	sleep 0.2

	for pid in $(pgrep -x plex_menu 2>/dev/null); do
		kill -KILL "$pid" >/dev/null 2>&1 || true
	done
fi

profile="${1:-debug}"

case "$profile" in
	debug)
		cargo build --bin plex_menu
		cargo_dir="target/debug"
		;;
	release)
		cargo build --release --bin plex_menu
		cargo_dir="target/release"
		;;
	*)
		cargo build --profile "$profile" --bin plex_menu
		cargo_dir="target/$profile"
		;;
esac

app_dir="$cargo_dir/Plex Menu.app"
contents_dir="$app_dir/Contents"
macos_dir="$contents_dir/MacOS"

mkdir -p "$macos_dir"
cp "$cargo_dir/plex_menu" "$macos_dir/plex_menu"
cp packaging/plex-menu/Info.plist "$contents_dir/Info.plist"

if [ "$(uname -s)" = "Darwin" ]; then
	codesign --force --deep --sign - "$app_dir"
fi

printf 'Built %s\n' "$app_dir"

open target/"$profile"/Plex\ Menu.app