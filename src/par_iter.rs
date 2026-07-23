/*
 * SPDX-FileCopyrightText: 2025 Tommaso Fontana
 * SPDX-FileCopyrightText: 2025 Luca Cappelletti
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use crate::*;
use rayon::iter::{plumbing::bridge_unindexed, ParallelIterator};

pub struct FrontierParIter<'a, T> {
    pub(crate) father: &'a Frontier<'a, T>,
}

impl<'a, T> FrontierParIter<'a, T> {
    pub fn new(father: &'a Frontier<T>) -> Self {
        FrontierParIter { father }
    }
}

impl<'a, T: Send + Sync> ParallelIterator for FrontierParIter<'a, T> {
    type Item = &'a T;

    fn drive_unindexed<C: rayon::iter::plumbing::UnindexedConsumer<Self::Item>>(
        self,
        consumer: C,
    ) -> C::Result {
        bridge_unindexed(FrontierProducer::new(self.father), consumer)
    }

    fn opt_len(&self) -> Option<usize> {
        None
    }
}
