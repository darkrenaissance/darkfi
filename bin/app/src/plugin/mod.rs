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

#[cfg(feature = "enable-plugin-darkirc")]
pub mod darkirc;
#[cfg(feature = "enable-plugin-darkirc")]
pub use darkirc::DarkIrcPtr;

#[cfg(feature = "enable-plugin-fud")]
pub mod fud;
#[cfg(feature = "enable-plugin-fud")]
pub use fud::FudPluginPtr as FudPtr;

#[cfg(feature = "enable-plugin-drk")]
pub mod drk;
#[cfg(feature = "enable-plugin-drk")]
pub use drk::DrkPluginPtr as DrkPtr;

#[cfg(feature = "enable-plugin-darkirc")]
pub use darkirc::DarkIrc;
#[cfg(feature = "enable-plugin-drk")]
pub use drk::DrkPlugin;
#[cfg(feature = "enable-plugin-fud")]
pub use fud::FudPlugin;
