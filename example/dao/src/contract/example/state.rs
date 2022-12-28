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

use std::any::Any;

use pasta_curves::pallas;

pub struct State {
    pub public_values: Vec<pallas::Base>,
}

impl State {
    pub fn new() -> Box<dyn Any + Send> {
        Box::new(Self { public_values: Vec::new() })
    }

    pub fn add_public_value(&mut self, public_value: pallas::Base) {
        self.public_values.push(public_value)
    }
    //
    pub fn public_exists(&self, public_value: &pallas::Base) -> bool {
        self.public_values.iter().any(|v| v == public_value)
    }
}
