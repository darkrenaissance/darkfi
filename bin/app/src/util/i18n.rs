/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use fluent::{concurrent::FluentBundle, FluentResource};
use parking_lot::RwLock;
use std::sync::Arc;
use unic_langid::langid;

pub type I18nResource = Arc<FluentResource>;

pub struct I18nBabelFish {
    bundle: RwLock<Arc<FluentBundle<I18nResource>>>,
}

impl I18nBabelFish {
    pub fn new(src: String, lang: &str) -> Self {
        let mut langs = vec![lang.parse().unwrap()];
        if lang != "en-US" {
            langs.push(langid!("en-US"));
        }

        let res = Arc::new(FluentResource::try_new(src).unwrap());
        let mut bundle = FluentBundle::new_concurrent(langs);
        bundle.add_resource(res).unwrap();
        Self { bundle: RwLock::new(Arc::new(bundle)) }
    }

    pub fn set(&self, other: Self) {
        let bundle = other.bundle.read();
        *self.bundle.write() = Arc::clone(&*bundle);
    }

    pub fn tr(&self, id: &str) -> Option<String> {
        let bundle = self.bundle.read();
        let msg = bundle.get_message(id)?;
        let patt = msg.value()?;
        // See FluentBundle::format_pattern()
        // this is where we can also pass args into the ftl str
        let mut errs = vec![];
        let res = bundle.format_pattern(&patt, None, &mut errs);
        Some(res.into_owned())
    }
}

impl Clone for I18nBabelFish {
    fn clone(&self) -> Self {
        let bundle = self.bundle.read();
        Self { bundle: RwLock::new(Arc::clone(&*bundle)) }
    }
}
