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

pub mod overlay2;
use overlay2::SledOverlay2;

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
    vec![&tree_1, &tree_2]
        .transaction(|trees| {
            for (i, tree) in trees.iter().enumerate() {
                tree.apply_batch(&batches[i])?;
            }

            Ok::<(), ConflictableTransactionError<sled::Error>>(())
        })
        .unwrap();
    db.flush()?;

    // Verify sled contains keys
    assert_eq!(tree_1.get(b"key_a")?, Some(b"val_a".into()));
    assert_eq!(tree_1.get(b"key_b")?, Some(b"val_b".into()));
    assert_eq!(tree_1.get(b"key_c")?, Some(b"val_c".into()));

    assert_eq!(tree_2.get(b"key_d")?, Some(b"val_d".into()));
    assert_eq!(tree_2.get(b"key_e")?, Some(b"val_e".into()));
    assert_eq!(tree_2.get(b"key_f")?, Some(b"val_f".into()));

    // Testing overlay2
    // Initialize database overlay
    let config = Config::new().temporary(true);
    let db = config.open()?;
    let mut overlay = SledOverlay2::new(&db);
    // Open trees in the overlay
    overlay.open_tree(TREE_1)?;
    overlay.open_tree(TREE_2)?;
    // We keep seperate trees for validation
    let tree_1 = db.open_tree(TREE_1)?;
    let tree_2 = db.open_tree(TREE_2)?;

    // Insert some values to the overlays
    overlay.insert(TREE_1, b"key_a", b"val_a")?;
    overlay.insert(TREE_1, b"key_b", b"val_b")?;
    overlay.insert(TREE_1, b"key_c", b"val_c")?;

    overlay.insert(TREE_2, b"key_d", b"val_d")?;
    overlay.insert(TREE_2, b"key_e", b"val_e")?;
    overlay.insert(TREE_2, b"key_f", b"val_f")?;

    // Verify they are in the overlay
    assert_eq!(overlay.get(TREE_1, b"key_a")?, Some(b"val_a".into()));
    assert_eq!(overlay.get(TREE_1, b"key_b")?, Some(b"val_b".into()));
    assert_eq!(overlay.get(TREE_1, b"key_c")?, Some(b"val_c".into()));

    assert_eq!(overlay.get(TREE_2, b"key_d")?, Some(b"val_d".into()));
    assert_eq!(overlay.get(TREE_2, b"key_e")?, Some(b"val_e".into()));
    assert_eq!(overlay.get(TREE_2, b"key_f")?, Some(b"val_f".into()));

    // Verify they are not in sled
    assert_eq!(tree_1.get(b"key_a")?, None);
    assert_eq!(tree_1.get(b"key_b")?, None);
    assert_eq!(tree_1.get(b"key_c")?, None);

    assert_eq!(tree_2.get(b"key_d")?, None);
    assert_eq!(tree_2.get(b"key_e")?, None);
    assert_eq!(tree_2.get(b"key_f")?, None);

    // Now execute all tree baches in the overlay
    assert_eq!(overlay.execute(), Ok(()));

    // Verify sled contains keys
    assert_eq!(tree_1.get(b"key_a")?, Some(b"val_a".into()));
    assert_eq!(tree_1.get(b"key_b")?, Some(b"val_b".into()));
    assert_eq!(tree_1.get(b"key_c")?, Some(b"val_c".into()));

    assert_eq!(tree_2.get(b"key_d")?, Some(b"val_d".into()));
    assert_eq!(tree_2.get(b"key_e")?, Some(b"val_e".into()));
    assert_eq!(tree_2.get(b"key_f")?, Some(b"val_f".into()));

    Ok(())
}
