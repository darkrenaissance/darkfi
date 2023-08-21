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

#[cfg(feature = "net")]
impl From<crate::net::channel::ChannelInfo> for tinyjson::JsonValue {
    fn from(info: crate::net::channel::ChannelInfo) -> tinyjson::JsonValue {
        tinyjson::JsonValue::Object(std::collections::HashMap::from([
            ("address".to_string(), tinyjson::JsonValue::String(info.address.to_string())),
            ("random_id".to_string(), tinyjson::JsonValue::Number(info.random_id.into())),
        ]))
    }
}

#[cfg(feature = "net")]
impl From<crate::net::dnet::MessageInfo> for tinyjson::JsonValue {
    fn from(info: crate::net::dnet::MessageInfo) -> tinyjson::JsonValue {
        tinyjson::JsonValue::Object(std::collections::HashMap::from([
            ("chan".to_string(), info.chan.into()),
            ("cmd".to_string(), tinyjson::JsonValue::String(info.cmd.clone())),
            ("time".to_string(), tinyjson::JsonValue::String(info.time.0.to_string())),
        ]))
    }
}

#[cfg(feature = "net")]
impl From<crate::net::dnet::DnetEvent> for tinyjson::JsonValue {
    fn from(event: crate::net::dnet::DnetEvent) -> tinyjson::JsonValue {
        match event {
            crate::net::dnet::DnetEvent::SendMessage(message_info) => message_info.into(),
        }
    }
}
