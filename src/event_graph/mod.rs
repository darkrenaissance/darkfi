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

use rand::{distributions::Alphanumeric, thread_rng, Rng};

pub mod events_queue;
pub mod model;
pub mod protocol_event;
pub mod view;

pub trait EventMsg {
    fn new() -> Self;
}

pub fn gen_id(len: usize) -> String {
    thread_rng().sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        events_queue::EventsQueue,
        model::{Event, EventId, Model},
        protocol_event::{Inv, InvItem, Seen, SeenPtr},
        view::View,
        EventMsg,
    };
    use crate::util::time::Timestamp;
    use darkfi_serial::{SerialDecodable, SerialEncodable};

    #[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
    struct TestEvent {
        pub nick: String,
        pub msg: String,
    }

    impl EventMsg for TestEvent {
        fn new() -> Self {
            Self { nick: "groot".to_string(), msg: "I am groot!!".to_string() }
        }
    }

    #[async_std::test]
    async fn event_graph_integration() {
        // Base structures
        let events_queue = EventsQueue::<TestEvent>::new();
        let mut model = Model::new(events_queue.clone());
        let _view = View::new(events_queue);

        // Buffers
        let _seen_event: SeenPtr<EventId> = Seen::new();
        let seen_inv: SeenPtr<EventId> = Seen::new();

        let seen_ids = Seen::new();
        // Keeps track of the events we received, but haven't read yet
        let mut unread_msgs = vec![];

        let test_event0 =
            TestEvent { nick: "brawndo".to_string(), msg: "Electrolytes".to_string() };
        let _test_event1 =
            TestEvent { nick: "camacho".to_string(), msg: "Shieeeeeeeet".to_string() };

        // We create an event and broadcast it
        let head_hash = model.get_head_hash();
        let event0 = Event {
            previous_event_hash: head_hash,
            action: test_event0,
            timestamp: Timestamp::current_time(),
        };

        // Simulate receiving the event
        assert!(seen_ids.push(&event0.hash()).await);
        // Simulate receiving the event again
        assert!(!seen_ids.push(&event0.hash()).await);

        // Add the event into the model
        model.add(event0.clone()).await;

        // Send inventory
        let inv0 = Inv { invs: vec![InvItem { hash: event0.hash() }] };
        // Simulate recieving the inventory
        assert!(seen_inv.push(&inv0.invs[0].hash).await);
        // Simulate recieving the inventory again
        assert!(!seen_inv.push(&inv0.invs[0].hash).await);

        // TODO: getdata (self.send_getdata(vec![inv_item.hash]).await?)

        // Add the event to the unread msgs vec
        unread_msgs.push(event0);

        // TODO: Simulate network behaviour, etc.
    }
}
