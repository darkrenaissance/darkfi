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

use serde::{Deserialize, Serialize};

pub const CONFIG_FILE: &str = "dnetview_config.toml";
pub const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../dnetview_config.toml");

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DnvConfig {
    pub nodes: Vec<Node>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Node {
    pub name: String,
    pub rpc_url: String,
    pub node_type: NodeType,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum NodeType {
    LILITH,
    NORMAL,
    CONSENSUS,
}
