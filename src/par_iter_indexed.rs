/*
 * SPDX-FileCopyrightText: 2025 Tommaso Fontana
 * SPDX-FileCopyrightText: 2025 Luca Cappelletti
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use crate::*;
use rayon::iter::{plumbing::*, IndexedParallelIterator};

impl<T: Send + Sync> IndexedParallelIterator for FrontierParIter<'_, T> {
    fn drive<C: Consumer<Self::Item>>(self, consumer: C) -> C::Result {
        bridge(self, consumer)
    }

    fn len(&self) -> usize {
        self.father.len()
    }

    fn with_producer<CB: ProducerCallback<Self::Item>>(self, callback: CB) -> CB::Output {
        // Drain every item, and then the vector only needs to free its buffer.
        callback.callback(FrontierProducer::new(self.father))
    }
}
