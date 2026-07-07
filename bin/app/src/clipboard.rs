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

#[cfg(target_os = "linux")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(target_os = "linux")]
static X11_CLIPBOARD: Mutex<Option<Arc<x11_clipboard::Clipboard>>> = Mutex::new(None);

#[cfg(target_os = "linux")]
fn get_clipboard() -> Option<Arc<x11_clipboard::Clipboard>> {
    let mut clipboard = X11_CLIPBOARD.lock().unwrap();
    if clipboard.is_none() {
        *clipboard = x11_clipboard::Clipboard::new().ok().map(Arc::new);
    }
    clipboard.clone()
}

pub fn get() -> Option<String> {
    #[cfg(target_os = "linux")]
    if let Some(clipboard) = get_clipboard() {
        if let Ok(bytes) = clipboard.load(
            clipboard.getter.atoms.clipboard,
            clipboard.getter.atoms.utf8_string,
            clipboard.getter.atoms.property,
            Duration::from_secs(2),
        ) {
            if let Ok(text) = String::from_utf8(bytes) {
                return Some(text)
            }
        }
    }

    miniquad::window::clipboard_get()
}

pub fn set(text: &str) {
    #[cfg(target_os = "linux")]
    if let Some(clipboard) = get_clipboard() {
        if clipboard.store(
            clipboard.setter.atoms.clipboard,
            clipboard.setter.atoms.utf8_string,
            text,
        ).is_ok() {
            return
        }
    }

    miniquad::window::clipboard_set(text);
}
