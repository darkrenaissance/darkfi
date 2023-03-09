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

use sled::{transaction::ConflictableTransactionError, Config, Transactional};

pub mod overlay;
use overlay::SledOverlay;

const TREE_1: &str = "_tree1";
const TREE_2: &str = "_tree2";

fn main() -> Result<(), sled::Error> {
    // Initialize database overlay
    let config = Config::new().temporary(true);
    let db = config.open()?;

    let tree_1 = db.open_tree(TREE_1)?;
    let tree_2 = db.open_tree(TREE_2)?;
    let mut overlay_1 = SledOverlay::new(&tree_1);
    let mut overlay_2 = SledOverlay::new(&tree_2);

    // Insert some values to the overlays
    overlay_1.insert(b"key_a", b"val_a")?;
    overlay_1.insert(b"key_b", b"val_b")?;
    overlay_1.insert(b"key_c", b"val_c")?;

    overlay_2.insert(b"key_d", b"val_d")?;
    overlay_2.insert(b"key_e", b"val_e")?;
    overlay_2.insert(b"key_f", b"val_f")?;

    // Verify they are in the overlays
    assert_eq!(overlay_1.get(b"key_a")?, Some(b"val_a".into()));
    assert_eq!(overlay_1.get(b"key_b")?, Some(b"val_b".into()));
    assert_eq!(overlay_1.get(b"key_c")?, Some(b"val_c".into()));

    assert_eq!(overlay_2.get(b"key_d")?, Some(b"val_d".into()));
    assert_eq!(overlay_2.get(b"key_e")?, Some(b"val_e".into()));
    assert_eq!(overlay_2.get(b"key_f")?, Some(b"val_f".into()));

    // Verify they are not in sled
    assert_eq!(tree_1.get(b"key_a")?, None);
    assert_eq!(tree_1.get(b"key_b")?, None);
    assert_eq!(tree_1.get(b"key_c")?, None);

    assert_eq!(tree_2.get(b"key_d")?, None);
    assert_eq!(tree_2.get(b"key_e")?, None);
    assert_eq!(tree_2.get(b"key_f")?, None);

    // Aggregate all the batches for writing
    let mut batches = vec![];
    batches.push(overlay_1.aggregate());
    batches.push(overlay_2.aggregate());

    // Now we write them to sled (this should be wrapped in a macro maybe)
    (&tree_1, &tree_2)
        .transaction(|(tree_1, tree_2)| {
            tree_1.apply_batch(&batches[0])?;
            tree_2.apply_batch(&batches[1])?;

            Ok::<(), ConflictableTransactionError<sled::Error>>(())
        })
        .unwrap();

    // Verify sled contains keys
    assert_eq!(tree_1.get(b"key_a")?, Some(b"val_a".into()));
    assert_eq!(tree_1.get(b"key_b")?, Some(b"val_b".into()));
    assert_eq!(tree_1.get(b"key_c")?, Some(b"val_c".into()));

    assert_eq!(tree_2.get(b"key_d")?, Some(b"val_d".into()));
    assert_eq!(tree_2.get(b"key_e")?, Some(b"val_e".into()));
    assert_eq!(tree_2.get(b"key_f")?, Some(b"val_f".into()));

    Ok(())
}
