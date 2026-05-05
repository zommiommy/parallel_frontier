/*
 * SPDX-FileCopyrightText: 2025 Tommaso Fontana
 * SPDX-FileCopyrightText: 2025 Luca Cappelletti
 *
 * SPDX-License-Identifier: Apache-2.0 OR LGPL-2.1-or-later
 */

use crate::*;
use rayon::iter::plumbing::{Folder, Producer, UnindexedProducer};
use std::sync::Arc;

/// Sequential iterator over the elements of a [`Frontier`].
///
/// The iterator walks the shards in order, skipping empty ones. It does not
/// know how to split itself; parallel iteration goes through
/// [`FrontierProducer`] instead.
pub struct FrontierIter<'a, T> {
    father: &'a Frontier<'a, T>,

    vec_idx_start: usize,
    value_idx_start: usize,

    // inclusive
    vec_idx_end: usize,
    // exclusive
    value_idx_end: usize,

    remaining: usize,
}

impl<T> core::fmt::Debug for FrontierIter<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FrontierIter")
            .field("vec_idx_start", &self.vec_idx_start)
            .field("value_idx_start", &self.value_idx_start)
            .field("vec_idx_end", &self.vec_idx_end)
            .field("value_idx_end", &self.value_idx_end)
            .field("remaining", &self.remaining)
            .finish()
    }
}

impl<'a, T> FrontierIter<'a, T> {
    pub fn new(father: &'a Frontier<T>) -> Self {
        let n_threads = father.number_of_threads();
        let last_len = father.as_ref().last().map_or(0, |v| v.len());
        FrontierIter {
            father,
            vec_idx_start: 0,
            value_idx_start: 0,
            vec_idx_end: n_threads.saturating_sub(1),
            value_idx_end: last_len,
            remaining: father.len(),
        }
    }

    pub fn len(&self) -> usize {
        self.remaining
    }

    pub fn is_empty(&self) -> bool {
        self.remaining == 0
    }
}

impl<T> core::iter::ExactSizeIterator for FrontierIter<'_, T> {}

impl<'a, T> core::iter::Iterator for FrontierIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let shards = self.father.as_ref();
        // Skip empty shards from the front. `remaining > 0` guarantees that a
        // non-empty shard exists within `[vec_idx_start..=vec_idx_end]`.
        while self.value_idx_start >= shards[self.vec_idx_start].len() {
            self.vec_idx_start += 1;
            self.value_idx_start = 0;
        }
        let result = &shards[self.vec_idx_start][self.value_idx_start];
        self.value_idx_start += 1;
        self.remaining -= 1;
        Some(result)
    }

    fn count(self) -> usize {
        self.remaining
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<T> core::iter::DoubleEndedIterator for FrontierIter<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let shards = self.father.as_ref();
        // Walk back over empty trailing shards. `remaining > 0` guarantees the
        // walk terminates before underflowing `vec_idx_end`.
        while self.value_idx_end == 0 {
            self.vec_idx_end -= 1;
            self.value_idx_end = shards[self.vec_idx_end].len();
        }
        self.value_idx_end -= 1;
        self.remaining -= 1;
        Some(&shards[self.vec_idx_end][self.value_idx_end])
    }
}

/// Rayon `Producer` and `UnindexedProducer` over a [`Frontier`].
///
/// Splits are pure range arithmetic on the half-open interval `[start, end)`
/// over the conceptual flattened sequence of all shards. The expensive
/// `(shard, offset)` lookup happens once, lazily, in [`Producer::into_iter`].
pub struct FrontierProducer<'a, T> {
    father: &'a Frontier<'a, T>,
    cumulative_lens: Arc<Vec<usize>>,
    start: usize,
    end: usize,
}

impl<T> core::fmt::Debug for FrontierProducer<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FrontierProducer")
            .field("start", &self.start)
            .field("end", &self.end)
            .field("cumulative_lens", &self.cumulative_lens)
            .finish()
    }
}

impl<'a, T> FrontierProducer<'a, T> {
    pub fn new(father: &'a Frontier<T>) -> Self {
        let cumulative_lens = Arc::new(
            father
                .as_ref()
                .iter()
                .map(|v| v.len())
                .scan(0, |acc, val| {
                    let res = *acc;
                    *acc += val;
                    Some(res)
                })
                .collect::<Vec<_>>(),
        );
        let end = father.len();
        FrontierProducer {
            father,
            cumulative_lens,
            start: 0,
            end,
        }
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Convert a flat index into a `(shard_idx, offset_in_shard)` pair.
fn locate(cumulative_lens: &[usize], idx: usize) -> (usize, usize) {
    match cumulative_lens.binary_search(&idx) {
        // Falls exactly on a shard boundary.
        Ok(vec_idx) => (vec_idx, 0),
        // Falls strictly inside a shard.
        Err(vec_idx) => {
            let vec_idx = vec_idx - 1;
            (vec_idx, idx - cumulative_lens[vec_idx])
        }
    }
}

impl<'a, T: Send + Sync> Producer for FrontierProducer<'a, T> {
    type Item = &'a T;
    type IntoIter = FrontierIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        let (vec_idx_start, value_idx_start) = locate(&self.cumulative_lens, self.start);
        let (vec_idx_end, value_idx_end) = locate(&self.cumulative_lens, self.end);
        FrontierIter {
            father: self.father,
            vec_idx_start,
            value_idx_start,
            vec_idx_end,
            value_idx_end,
            remaining: self.end - self.start,
        }
    }

    fn split_at(self, index: usize) -> (Self, Self) {
        let mid = self.start + index;
        debug_assert!(mid <= self.end);
        (
            Self {
                father: self.father,
                cumulative_lens: self.cumulative_lens.clone(),
                start: self.start,
                end: mid,
            },
            Self {
                father: self.father,
                cumulative_lens: self.cumulative_lens,
                start: mid,
                end: self.end,
            },
        )
    }
}

impl<'a, T: Send + Sync> UnindexedProducer for FrontierProducer<'a, T> {
    type Item = &'a T;

    fn split(self) -> (Self, Option<Self>) {
        if self.len() < 2 {
            return (self, None);
        }
        let mid = (self.start + self.end) / 2;
        (
            Self {
                father: self.father,
                cumulative_lens: self.cumulative_lens.clone(),
                start: self.start,
                end: mid,
            },
            Some(Self {
                father: self.father,
                cumulative_lens: self.cumulative_lens,
                start: mid,
                end: self.end,
            }),
        )
    }

    fn fold_with<F>(self, folder: F) -> F
    where
        F: Folder<Self::Item>,
    {
        folder.consume_iter(Producer::into_iter(self))
    }
}
