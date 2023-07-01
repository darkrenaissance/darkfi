/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

//! TODO: this is just the foundation layout, so we can complete
//! the basic validator. We will use pallas::Base::zero() everywhere,
//! since we just want to simulate its functionality. After layout is
//! complete, the proper pid functionality will be implemented.

use darkfi_sdk::pasta::pallas;

/// Return 2-term target approximation sigma coefficients,
/// corresponding to current slot consensus state.
pub fn current_sigmas() -> (pallas::Base, pallas::Base) {
    (pallas::Base::zero(), pallas::Base::zero())
}

/// Return 2-term target approximation sigma coefficients,
/// corresponding to provided slot consensus state.
pub fn slot_sigmas() -> (pallas::Base, pallas::Base) {
    (pallas::Base::zero(), pallas::Base::zero())
}
