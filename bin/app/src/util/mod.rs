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

#[cfg(target_os = "linux")]
use colored::Colorize;
use std::time::{SystemTime, UNIX_EPOCH};

pub mod i18n;
mod rt;
pub use rt::{AsyncRuntime, ExecutorPtr};

/// Use src/util/time.rs Timestamp instead of this.
pub fn unixtime() -> u64 {
    let timest = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    assert!(timest < std::u64::MAX as u128);
    timest as u64
}

pub fn spawn_thread<F, T, S>(name: S, f: F) -> std::thread::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
    S: Into<String>,
{
    std::thread::Builder::new().name(name.into()).spawn(f).unwrap()
}

#[allow(dead_code)]
pub fn ansi_texture(width: usize, height: usize, data: &Vec<u8>) -> String {
    let mut out = String::new();

    out.push('┌');
    for _ in 0..width {
        out.push('─');
    }
    out.push('┐');
    out.push('\n');

    for i in 0..height {
        out.push('│');
        for j in 0..width {
            let idx = 4 * (i * width + j);

            #[cfg(target_os = "android")]
            {
                let a = data[idx + 3];

                if a > 204 {
                    out.push('█');
                } else if a > 153 {
                    out.push('▓');
                } else if a > 102 {
                    out.push('▒');
                } else if a > 51 {
                    out.push('░');
                } else {
                    out.push(' ');
                }
            }

            #[cfg(target_os = "linux")]
            {
                let r = data[idx];
                let g = data[idx + 1];
                let b = data[idx + 2];
                let a = data[idx + 3];

                let r = ((a as f32 * r as f32) / 255.) as u8;
                let g = ((a as f32 * g as f32) / 255.) as u8;
                let b = ((a as f32 * b as f32) / 255.) as u8;

                let val = "█".truecolor(r, g, b).to_string();
                out.push_str(&val);
            }
        }
        out.push('│');
        out.push('\n');
    }

    out.push('└');
    for _ in 0..width {
        out.push('─');
    }
    out.push('┘');
    out.push('\n');

    out
}

pub struct TupleIterStruct3<I1, I2, I3> {
    idx: usize,
    i1: I1,
    i2: I2,
    i3: I3,
}

impl<I1, I2, I3> Iterator for TupleIterStruct3<I1, I2, I3>
where
    I1: Iterator,
    I2: Iterator,
    I3: Iterator,
{
    type Item = (usize, I1::Item, I2::Item, I3::Item);

    fn next(&mut self) -> Option<Self::Item> {
        let Some(x1) = self.i1.next() else { return None };
        let Some(x2) = self.i2.next() else { return None };
        let Some(x3) = self.i3.next() else { return None };

        let res = (self.idx, x1, x2, x3);
        self.idx += 1;

        Some(res)
    }
}

#[allow(dead_code)]
pub fn zip3<X1, X2, X3, I1, I2, I3>(i1: I1, i2: I2, i3: I3) -> TupleIterStruct3<I1, I2, I3>
where
    I1: Iterator<Item = X1>,
    I2: Iterator<Item = X2>,
    I3: Iterator<Item = X3>,
{
    TupleIterStruct3 { idx: 0, i1, i2, i3 }
}

pub struct TupleIterStruct4<I1, I2, I3, I4> {
    idx: usize,
    i1: I1,
    i2: I2,
    i3: I3,
    i4: I4,
}

impl<I1, I2, I3, I4> Iterator for TupleIterStruct4<I1, I2, I3, I4>
where
    I1: Iterator,
    I2: Iterator,
    I3: Iterator,
    I4: Iterator,
{
    type Item = (usize, I1::Item, I2::Item, I3::Item, I4::Item);

    fn next(&mut self) -> Option<Self::Item> {
        let Some(x1) = self.i1.next() else { return None };
        let Some(x2) = self.i2.next() else { return None };
        let Some(x3) = self.i3.next() else { return None };
        let Some(x4) = self.i4.next() else { return None };

        let res = (self.idx, x1, x2, x3, x4);
        self.idx += 1;

        Some(res)
    }
}

#[allow(dead_code)]
pub fn zip4<X1, X2, X3, X4, I1, I2, I3, I4>(
    i1: I1,
    i2: I2,
    i3: I3,
    i4: I4,
) -> TupleIterStruct4<I1, I2, I3, I4>
where
    I1: Iterator<Item = X1>,
    I2: Iterator<Item = X2>,
    I3: Iterator<Item = X3>,
    I4: Iterator<Item = X4>,
{
    TupleIterStruct4 { idx: 0, i1, i2, i3, i4 }
}

#[allow(dead_code)]
pub fn enumerate<X>(v: Vec<X>) -> impl Iterator<Item = (usize, X)> {
    v.into_iter().enumerate()
}
#[allow(dead_code)]
pub fn enumerate_ref<X>(v: &Vec<X>) -> impl Iterator<Item = (usize, &X)> {
    v.iter().enumerate()
}
pub fn enumerate_mut<X>(v: &mut Vec<X>) -> impl Iterator<Item = (usize, &mut X)> {
    v.iter_mut().enumerate()
}
