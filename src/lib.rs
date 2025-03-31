/*
 * SPDX-FileCopyrightText: 2025 Tommaso Fontana
 *
 * SPDX-License-Identifier: Apache-2.0 OR LGPL-2.1-or-later
 */

mod frontier;
mod iter;
mod par_iter;
mod par_iter_indexed;

pub mod prelude {
    pub use crate::frontier::*;
    pub use crate::iter::*;
    pub use crate::par_iter::*;
    pub use rayon::prelude::*;
}
