/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use crate::util::time::NanoTimestamp;

macro_rules! degev {
      ($self:expr, $event_name:ident, $($code:tt)*) => {
          {
              if *$self.event_graph.deg_enabled.read().await {
                  let event = DegEvent::$event_name(deg::$event_name $($code)*);
                  $self.event_graph.deg_notify(event).await;
              }
          }
      };
  }
pub(crate) use degev;

#[derive(Clone, Debug)]
pub struct MessageInfo {
    pub info: Vec<String>,
    pub cmd: String,
    pub time: NanoTimestamp,
}

// Needed by the degev!() macro
pub type SendMessage = MessageInfo;
pub type RecvMessage = MessageInfo;

#[derive(Clone, Debug)]
pub enum DegEvent {
    SendMessage(MessageInfo),
    RecvMessage(MessageInfo),
}
