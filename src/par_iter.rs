/*
 * SPDX-FileCopyrightText: 2025 Tommaso Fontana
 * SPDX-FileCopyrightText: 2025 Luca Cappelletti
 *
 * SPDX-License-Identifier: Apache-2.0 OR LGPL-2.1-or-later
 */

use crate::prelude::*;
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

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: rayon::iter::plumbing::UnindexedConsumer<Self::Item>,
    {
        bridge_unindexed(FrontierIter::new(self.father), consumer)
    }

    fn opt_len(&self) -> Option<usize> {
        None
    }
}
