/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{fs::File, io::Write};

/// Adds a temporary workaround for [an issue] with the Rust compiler and Android when
/// compiling sqlite3 bundled C code.
///
/// The Android NDK used to include `libgcc` for unwind support (which is required by Rust
/// among others). From NDK r23, `libgcc` is removed, replaced by LLVM's `libunwind`.
/// However, `libgcc` was ambiently providing other compiler builtins, one of which we
/// require: `__extenddftf2` for software floating-point emulation. This is used by SQLite
/// (via the `rusqlite` crate), which defines a `LONGDOUBLE_TYPE` type as `long double`.
///
/// Rust uses a `compiler-builtins` crate that does not provide `__extenddftf2` because
/// it involves floating-point types that are not supported by Rust.
///
/// The workaround comes from [this Mozilla PR]: we tell Cargo to statically link the
/// builtins from the Clang runtime provided inside the NDK, to provide this symbol.
///
/// See also this [zcash issue] and [their workaround].
///
/// [an issue]: https://github.com/rust-lang/rust/issues/109717
/// [this Mozilla PR]: https://github.com/mozilla/application-services/pull/5442
/// [unsupported]: https://github.com/rust-lang/compiler-builtins#unimplemented-functions
/// [zcash issue]: https://github.com/zcash/librustzcash/issues/800
/// [their workaround]: https://github.com/Electric-Coin-Company/zcash-android-wallet-sdk/blob/88058c63461f2808efc953af70db726b9f36f9b9/backend-lib/build.rs
fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not set");
    let target_arch =
        std::env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH not set");
    //println!("cargo:warning={target_arch}");

    // Add some useful debug info directly into the app itself
    let mut f = File::create("src/build_info.rs").unwrap();
    writeln!(f, "pub const TARGET_OS: &'static str = \"{target_os}\";").unwrap();
    writeln!(f, "pub const TARGET_ARCH: &'static str = \"{target_arch}\";").unwrap();

    if target_os == "android" {
        // Since we run this inside a container, we can just hardcore the paths directly
        println!("cargo:rustc-link-search=/opt/android-ndk-r25/toolchains/llvm/prebuilt/linux-x86_64/lib64/clang/14.0.6/lib/linux/");
        match target_arch.as_str() {
            "aarch64" => println!("cargo:rustc-link-lib=static=clang_rt.builtins-aarch64-android"),
            "arm" => println!("cargo:rustc-link-lib=static=clang_rt.builtins-arm-android"),
            "i686" => println!("cargo:rustc-link-lib=static=clang_rt.builtins-i686-android"),
            "x86_64" => println!("cargo:rustc-link-lib=static=clang_rt.builtins-x86_64-android"),
            // Maybe this should panic instead
            _ => println!(
                "cargo:warning='leaving linker args for {target_os}:{target_arch} unchanged"
            ),
        }
    }
}
