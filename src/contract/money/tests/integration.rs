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

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use darkfi_sdk::blockchain::expected_reward;

#[test]
fn money_integration() -> Result<()> {
    smol::block_on(async {
        init_logger();

        use Holder::{Alice, Bob};

        let mut th = TestHarness::new(&[Alice, Bob], true).await?;

        // Mine 2 blocks each
        th.generate_block_all(&Alice).await?;
        th.generate_block_all(&Alice).await?;
        th.generate_block_all(&Bob).await?;
        th.generate_block_all(&Bob).await?;

        // Assert correct rewards
        assert_eq!(th.coins(&Alice).len(), 2);
        assert_eq!(th.coins(&Bob).len(), 2);
        assert_eq!(th.coins(&Alice)[0].note.value, expected_reward(1));
        assert_eq!(th.coins(&Alice)[1].note.value, expected_reward(2));
        assert_eq!(th.coins(&Bob)[0].note.value, expected_reward(3));
        assert_eq!(th.coins(&Bob)[1].note.value, expected_reward(4));

        let block_height = 4;
        let native_token = th.coins(&Alice)[0].note.token_id;

        // Alice transfers her first block reward to Bob
        let transfer_amount = expected_reward(1);
        th.transfer_to_all(transfer_amount, &Alice, &Bob, native_token, block_height).await?;

        // Alice: 1 coin (fee change from reward(2))
        // Bob:   3 coins (reward(3) + reward(4) + received reward(1))
        assert_eq!(th.coins(&Alice).len(), 1);
        assert_eq!(th.coins(&Bob).len(), 3);
        assert_eq!(th.coins(&Bob)[2].note.value, transfer_amount);

        // Thanks for reading
        Ok(())
    })
}
