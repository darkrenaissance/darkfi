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

use crate::{
    runtime::vm_runtime::{ContractSection, Env},
    Error, Result,
};

/// Return an error if the current Env section is not in the sections list.
pub(super) fn acl_allow(env: &Env, sections: &[ContractSection]) -> Result<()> {
    if !sections.contains(&env.contract_section) {
        return Err(Error::WasmFunctionAclDenied)
    }

    Ok(())
}
