#!/bin/sh

set -eu

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

printf 'Built %s\n' "$app_dir"
