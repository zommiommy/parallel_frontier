/*
 * SPDX-FileCopyrightText: 2025 Tommaso Fontana
 *
 * SPDX-License-Identifier: Apache-2.0 OR LGPL-2.1-or-later
 */

use crate::prelude::*;
use rayon::iter::{plumbing::*, IndexedParallelIterator};

impl<'a, T: Send + Sync> IndexedParallelIterator for FrontierParIter<'a, T> {
    fn drive<C>(self, consumer: C) -> C::Result
    where
        C: Consumer<Self::Item>,
    {
        bridge(self, consumer)
    }

    fn len(&self) -> usize {
        self.father.len()
    }

    fn with_producer<CB>(self, callback: CB) -> CB::Output
    where
        CB: ProducerCallback<Self::Item>,
    {
        // Drain every item, and then the vector only needs to free its buffer.
        callback.callback(self.father.iter())
    }
}
