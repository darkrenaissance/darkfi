#!/bin/sh
set -e

if [ "$(id -u)" != 0 ]; then
	SUDO="${SUDO:-$(command -v sudo)}"
else
	SUDO="${SUDO:-}"
fi

brew_sh="https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh"

setup_mac() {
	if ! command -v brew >/dev/null; then
		echo "brew not found, installing..." >&2
		bash -c "$(curl -fL "${brew_sh}")" || return 1
	fi

	for i in cmake gcc jq pkgconf llvm@13; do
		echo "Installing $i with brew..." >&2
		brew install "$i" || return 1
	done
}

setup_apt() {
	apt_deps="git make jq gcc pkg-config libmpg123-dev"
	$1 install $apt_deps || return 1
}

setup_pacman() {
	pacman_deps="git make jq gcc pkgconf mpg123"
	$1 -Sy $pacman_deps || return 1
}

setup_xbps() {
	xbps_deps="git make jq gcc pkg-config"
	$1 -S $xbps_deps || return 1
}

setup_dnf() {
	dnf_deps="git make jq gcc pkg-config findutils lato-fonts"
	$1 install -y $dnf_deps || return 1
}

setup_apk() {
	apk_deps="git make jq gcc musl-dev"
	$1 add $apk_deps || return 1
}

setup_zypper() {
	zypper_deps="git make jq gcc pkg-config findutils"
	$1 install -y $zypper_deps || return 1
}

setup_emerge() {
	emerge_deps="dev-vcs/git app-misc/jq dev-util/pkgconf"
	$1 $emerge_deps || return 1
}

setup_pkg() {
	pkg_deps="git bash jq gcc findutils cantarell-fonts devel/pkgconf gmake devel/automake pulseaudio-module-sndio"
	$1 install -y $pkg_deps || return 1
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
	if command -v pkg; then
		echo "Setting up for pkg/BSD" >&2
		setup_pkg "$SUDO $(command -v pkg)" || exit 1
		echo "Dependencies installed!" >&2
		cat <<'ENDOFCAT' >&2
=======================
Few more things needed:
Install rust from https://www.rust-lang.org/tools/install
Execute: rustup target add wasm32-unknown-unknown
And apply few patches ...
cd ..
git clone --recurse-submodules -j8 https://github.com/stainless-steel/mpg123-sys
cd ./mpg123-sys
sed -e's/$(RM)/rm -f/g' -i.bak  source/Makefile.in

patch -p0 build.rs <<'EOF'
43a44
>             .arg(&format!("--with-audio=sndio"))
EOF

cargo build  # to see if it works fine

cd ../darkfi

patch -p0 Cargo.toml <<'EOF'
329a330
> mpg123-sys = { path = "../mpg123-sys" }
EOF

gmake test  # no errors expected
gmake
ls -al darkfid ircd dnetview faucetd vanityaddr  # list of built executables
ENDOFCAT
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
