#!/bin/sh

set -eu

profile="${1:-debug}"

case "$profile" in
	debug)
		cargo build --bin space_timer
		cargo_dir="target/debug"
		;;
	release)
		cargo build --release --bin space_timer
		cargo_dir="target/release"
		;;
	*)
		cargo build --profile "$profile" --bin space_timer
		cargo_dir="target/$profile"
		;;
esac

app_dir="$cargo_dir/Space Timer.app"
contents_dir="$app_dir/Contents"
macos_dir="$contents_dir/MacOS"

mkdir -p "$macos_dir"
cp "$cargo_dir/space_timer" "$macos_dir/space_timer"
cp packaging/space_timer/Info.plist "$contents_dir/Info.plist"

printf 'Built %s\n' "$app_dir"
