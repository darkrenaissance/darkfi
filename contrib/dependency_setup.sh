#!/bin/sh
set -e

if [ "$UID" != 0 ]; then
	SUDO="$(command -v sudo)"
else
	SUDO=""
fi

brew_sh="https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh"

setup_mac() {
	if ! command -v brew >/dev/null; then
		echo "brew not found, installing..." >&2
		bash -c "$(curl -fL "${brew_sh}")" || return 1
	fi

	for i in cmake gcc jq pkgconf llvm@13 freetype expat; do
		echo "Installing $i with brew..." >&2
		brew install "$i" || return 1
	done
}

setup_apt() {
	APTGET="$SUDO $1"

	$APTGET update || return 1
	$APTGET install -y build-essential cmake jq wget pkg-config \
		clang libclang-dev llvm-dev libudev-dev libfreetype6-dev \
		libexpat1-dev || return 1
}

setup_xbps() {
	XBPS="$SUDO $1"

	$XBPS -S base-devel cmake wget expat-devel freetype-devel \
		fontconfig-devel jq openssl-devel clang libclang llvm \
		libllvm12 libgudev-devel
}

case "$(uname -s)" in
Linux)
	if command -v apt >/dev/null; then
		echo "Setting up for apt" >&2
		setup_apt "$(command -v apt)" || exit 1
		echo "Dependencies installed!" >&2
		exit 0
	fi

	if command -v apt-get >/dev/null; then
		echo "Setting up for apt-get" >&2
		setup_apt "$(command -v apt-get)" || exit 1
		echo "Dependencies installed!" >&2
		exit 0
	fi

	if command -v xbps-install; then
		echo "Setting up for xbps" >&2
		setup_xbps "$(command -v xbps-install)" || exit 1
		echo "Dependencies installed!" >&2
		exit 0
	fi

	echo "Error: Could not recognize your package manager." >&2
	exit 1
	;;

Darwin)
	echo "Setting up for OSX" >&2
	setup_mac || exit 1
	echo "Dependencies installed!" >&2
	exit 0
	;;

*|"")
	echo "Unsupported OS, sorry." >&2
	exit 1
	;;
esac
