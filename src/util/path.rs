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

use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use crate::{Error, Result};

#[cfg(target_family = "unix")]
mod home_dir_impl {
    use std::{
        env,
        ffi::{CStr, OsString},
        mem,
        os::unix::prelude::OsStringExt,
        path::PathBuf,
        ptr,
    };

    /// Returns the path to the user's home directory.
    /// Use `$HOME`, fallbacks to `libc::getpwuid_r`, otherwise `None`.
    pub fn home_dir() -> Option<PathBuf> {
        env::var_os("HOME")
            .and_then(|h| if h.is_empty() { None } else { Some(h) })
            .or_else(|| unsafe { home_fallback() })
            .map(PathBuf::from)
    }

    /// Get the home directory from the passwd entry of the current user using
    /// `getpwuid_r(3)`. If it manages, returns an `OsString`, otherwise returns `None`.
    unsafe fn home_fallback() -> Option<OsString> {
        let amt = match libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) {
            n if n < 0 => 512_usize,
            n => n as usize,
        };

        let mut buf = Vec::with_capacity(amt);
        let mut passwd: libc::passwd = mem::zeroed();
        let mut result = ptr::null_mut();

        let r = libc::getpwuid_r(
            libc::getuid(),
            &mut passwd,
            buf.as_mut_ptr(),
            buf.capacity(),
            &mut result,
        );

        match r {
            0 if !result.is_null() => {
                let ptr = passwd.pw_dir as *const _;
                let bytes = CStr::from_ptr(ptr).to_bytes();
                if bytes.is_empty() {
                    return None
                }

                Some(OsStringExt::from_vec(bytes.to_vec()))
            }

            _ => None,
        }
    }
}

#[cfg(target_family = "windows")]
mod home_dir_impl {
    use std::{env, path::PathBuf};

    pub fn home_dir() -> Option<PathBuf> {
        env::var_os("APPDATA").map(PathBuf::from)
    }
}

pub use home_dir_impl::home_dir;

/// Returns `$XDG_CONFIG_HOME`, `$HOME/.config`, or `None`.
pub fn config_dir() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .and_then(is_absolute_path)
        .or_else(|| home_dir().map(|h| h.join(".config")))
}

fn is_absolute_path(path: OsString) -> Option<PathBuf> {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Some(path)
    } else {
        None
    }
}

pub fn expand_path(path: &str) -> Result<PathBuf> {
    let ret: PathBuf;

    if path.starts_with("~/") {
        if let Some(homedir) = home_dir() {
            let remains = PathBuf::from(path.strip_prefix("~/").unwrap());
            ret = [homedir, remains].iter().collect();
        } else {
            panic!("Could not fetch path for home directory");
        }
    } else if path.starts_with('~') {
        if let Some(homedir) = home_dir() {
            ret = homedir
        } else {
            panic!("Could not fetch path for home directory");
        }
    } else {
        ret = PathBuf::from(path);
    }

    Ok(ret)
}

/// Join a path with `config_dir()/darkfi`.
pub fn join_config_path(file: &Path) -> Result<PathBuf> {
    let mut path = PathBuf::new();
    let dfi_path = Path::new("darkfi");

    if let Some(v) = config_dir() {
        path.push(v);
    }

    path.push(dfi_path);
    path.push(file);

    Ok(path)
}

pub fn get_config_path(arg: Option<String>, fallback: &str) -> Result<PathBuf> {
    if let Some(a) = arg {
        expand_path(&a)
    } else {
        join_config_path(&PathBuf::from(fallback))
    }
}

pub fn load_keypair_to_str(path: PathBuf) -> Result<String> {
    if Path::new(&path).exists() {
        let key = fs::read(&path)?;
        let str_buff = std::str::from_utf8(&key)?;
        Ok(str_buff.to_string())
    } else {
        println!("Could not parse keypair path");
        Err(Error::KeypairPathNotFound)
    }
}
