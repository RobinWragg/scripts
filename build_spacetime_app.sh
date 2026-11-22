#!/bin/sh

set -eu

profile="${1:-debug}"

case "$profile" in
	debug)
		cargo build --bin spacetime
		cargo_dir="target/debug"
		;;
	release)
		cargo build --release --bin spacetime
		cargo_dir="target/release"
		;;
	*)
		cargo build --profile "$profile" --bin spacetime
		cargo_dir="target/$profile"
		;;
esac

app_dir="$cargo_dir/Spacetime.app"
contents_dir="$app_dir/Contents"
macos_dir="$contents_dir/MacOS"

mkdir -p "$macos_dir"
cp "$cargo_dir/spacetime" "$macos_dir/spacetime"
cp packaging/spacetime/Info.plist "$contents_dir/Info.plist"

printf 'Built %s\n' "$app_dir"
