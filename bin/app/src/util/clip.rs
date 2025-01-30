/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

pub struct Clipboard {
    #[cfg(not(target_os = "android"))]
    clip: arboard::Clipboard,
}

impl Clipboard {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_os = "android"))]
            clip: arboard::Clipboard::new().unwrap(),
        }
    }

    pub fn get(&mut self) -> Option<String> {
        #[cfg(target_os = "android")]
        return miniquad::window::clipboard_get();

        #[cfg(not(target_os = "android"))]
        return self.clip.get_text().ok();
    }

    pub fn set(&mut self, data: &str) {
        #[cfg(target_os = "android")]
        return miniquad::window::clipboard_set(data);

        #[cfg(not(target_os = "android"))]
        return self.clip.set_text(data).unwrap();
    }
}
