#!/bin/sh
set -e

if [ "$(id -u)" != 0 ]; then
	if command -v sudo; then
		SUDO="${SUDO:-$(command -v sudo)}"
	elif command -v doas; then
		SUDO="${SUDO:-$(command -v doas)}"
    else
		echo "Please run this script as root!" >&2
		exit
    fi
else
	SUDO="${SUDO:-}"
fi

brew_sh="https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh"

setup_mac() {
	if ! command -v brew >/dev/null; then
		echo "brew not found, installing..." >&2
		bash -c "$(curl -fL "${brew_sh}")" || return 1
	fi

	for i in cmake gcc pkgconf llvm@13; do
		echo "Installing $i with brew..." >&2
		brew install "$i" || return 1
	done
}

setup_apt() {
	apt_deps="git cmake make gcc g++ pkg-config libasound2-dev libclang-dev libfontconfig1-dev liblzma-dev libssl-dev libsqlcipher-dev libsqlite3-dev wabt"
	$1 install $apt_deps || return 1
}

setup_pacman() {
	pacman_deps="git cmake make gcc pkgconf alsa-lib openssl sqlcipher wabt"
	$1 -Sy $pacman_deps || return 1
}

setup_xbps() {
	xbps_deps="git make gcc pkg-config alsa-lib-devel openssl-devel sqlcipher-devel wabt"
	$1 -S $xbps_deps || return 1
}

setup_dnf() {
	dnf_deps="git make gcc pkg-config findutils lato-fonts fontconfig-devel perl-FindBin perl-File-Compare alsa-lib-devel python3-devel alsa-lib-devel openssl-devel sqlcipher-devel libsq3-devel wabt"
	$1 install -y $dnf_deps || return 1
}

setup_apk() {
	apk_deps="git make gcc musl-dev pkgconfig alsa-lib-dev openssl-dev sqlcipher-dev"
	$1 add $apk_deps || return 1
}

setup_zypper() {
	zypper_deps="git make gcc pkg-config alsa-devel openssl-devel sqlcipher-devel"
	$1 install -y $zypper_deps || return 1
}

setup_emerge() {
	emerge_deps="dev-vcs/git media-libs/alsa-lib dev-db/sqlcipher"
	$1 $emerge_deps || return 1
}

setup_pkg() {
	pkg_deps="git bash gcc findutils cantarell-fonts gmake devel/automake rust wabt llvm cmake sqlcipher pkgconf python python3"
	$1 install -y $pkg_deps || return 1
}

setup_pkg_add() {
	pkg_add_deps="git bash gcc-11.2.0p6 findutils cantarell-fonts gmake automake-1.15.1"
	$1 -I $pkg_add_deps || return 1
}

setup_pkgin() {
	pkgin_deps="git bash gcc12 findutils cantarell-fonts pkgconf gmake automake"
	$1 -y install $pkgin_deps || return 1
}

case "$(uname -s)" in
Linux)
	if command -v apt >/dev/null; then
		echo "Setting up for apt" >&2
		setup_apt "$SUDO $(command -v apt)" || exit 1
		echo "Dependencies installed!" >&2
		exit 0
	fi

	if command -v apt-get >/dev/null; then
		echo "Setting up for apt-get" >&2
		setup_apt "$SUDO $(command -v apt-get)" || exit 1
		echo "Dependencies installed!" >&2
		exit 0
	fi

	if command -v pacman; then
		echo "Setting up for pacman" >&2
		setup_pacman "$SUDO $(command -v pacman)" || exit 1
		echo "Dependencies installed!" >&2
		exit 0
	fi

	if command -v xbps-install; then
		echo "Setting up for xbps" >&2
		setup_xbps "$SUDO $(command -v xbps-install)" || exit 1
		echo "Dependencies installed!" >&2
		exit 0
	fi


	if command -v dnf; then
		echo "Setting up for dnf" >&2
		setup_dnf "$SUDO $(command -v dnf)" || exit 1
		echo "Dependencies installed!" >&2
		exit 0
	fi

	if command -v apk; then
		echo "Setting up for apk" >&2
		setup_apk "$SUDO $(command -v apk)" || exit 1
		echo "Dependencies installed!" >&2
		exit 0
	fi

	if command -v zypper; then
		echo "Setting up for zypper" >&2
		setup_zypper "$SUDO $(command -v zypper)" || exit 1
		echo "Dependencies installed!" >&2
		exit 0
	fi

	if command -v emerge; then
		echo "Setting up for emerge" >&2
		setup_emerge "$SUDO $(command -v emerge)" || exit 1
		echo "Dependencies installed!" >&2
		exit 0
	fi

	echo "Error: Could not recognize your package manager." >&2
	exit 1
	;;
*BSD*)
	if command -v pkgin; then
		echo "Setting up for pkgin/NetBSD" >&2
		setup_pkgin "$SUDO $(command -v pkgin)" || exit 1
                exit 0
	elif command -v pkg; then
		echo "Setting up for pkg/FreeBSD" >&2
		setup_pkg "$SUDO $(command -v pkg)" || exit 1
                exit 0
	elif command -v pkg_add; then
		echo "Setting up for pkg_add/OpenBSD" >&2
		setup_pkg_add "$SUDO $(command -v pkg_add)" || exit 1
		echo "Rust support is not yet ready for OpenBSD, see https://github.com/rust-lang/rustup/issues/2168#issuecomment-1505185711"
		echo "You may try to compile rustc and cargo yourself or get the latest with:"
		echo "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y --default-toolchain stable --default-host x86_64-unknown-openbsd"
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

""|*)
	echo "Unsupported OS, sorry." >&2
	exit 1
	;;
esac
