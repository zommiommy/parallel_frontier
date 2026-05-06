/*
 * SPDX-FileCopyrightText: 2025 Tommaso Fontana
 * SPDX-FileCopyrightText: 2025 Luca Cappelletti
 *
 * SPDX-License-Identifier: Apache-2.0 OR LGPL-2.1-or-later
 */

extern crate parallel_frontier;

use parallel_frontier::*;
use rayon::iter::plumbing::{Producer, UnindexedProducer};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;

// --------------------------------------------------------------------------
// Constructors
// --------------------------------------------------------------------------

#[test]
fn default_matches_new() {
    let a: Frontier<usize> = Frontier::default();
    let b: Frontier<usize> = Frontier::new();
    assert_eq!(a, b);
    assert_eq!(a.number_of_threads(), b.number_of_threads());
}

#[test]
fn with_capacity_creates_empty_frontier() {
    let f: Frontier<usize> = Frontier::with_capacity(64);
    assert!(f.is_empty());
    assert!(f.number_of_threads() >= 1);
}

#[test]
fn with_threads_capacity_uses_pool_size() {
    let pool = ThreadPoolBuilder::default().num_threads(2).build().unwrap();
    let f: Frontier<usize> = Frontier::with_threads(&pool, Some(64));
    assert_eq!(2, f.number_of_threads());
    assert!(f.is_empty());
}

// --------------------------------------------------------------------------
// Push / pop / state
// --------------------------------------------------------------------------

#[test]
fn push_outside_pool_targets_shard_zero() {
    // self.threads = Some(pool) but call site is the main thread (no rayon
    // pool active), exercising the `0` branch of get_current_thread_index.
    let pool = ThreadPoolBuilder::default().num_threads(2).build().unwrap();
    let f: Frontier<usize> = Frontier::with_threads(&pool, None);
    f.push(1);
    f.push(2);
    assert_eq!(*f.as_ref()[0], vec![1, 2]);
    assert!(f.as_ref()[1].is_empty());
}

#[test]
fn push_inside_assigned_pool_uses_thread_index() {
    let pool = ThreadPoolBuilder::default().num_threads(2).build().unwrap();
    let f: Frontier<usize> = Frontier::with_threads(&pool, None);
    pool.install(|| {
        (0..200).into_par_iter().for_each(|i| f.push(i));
    });
    assert_eq!(200, f.len());
}

#[test]
#[should_panic(expected = "Parallel frontier called from external thread pool")]
fn push_from_foreign_pool_panics() {
    let inner = ThreadPoolBuilder::default().num_threads(1).build().unwrap();
    let outer = ThreadPoolBuilder::default().num_threads(1).build().unwrap();
    let f: Frontier<usize> = Frontier::with_threads(&inner, None);
    outer.install(|| f.push(0));
}

#[test]
fn push_on_thread_targets_chosen_shard() {
    let f: Frontier<usize> = Frontier::new();
    if f.number_of_threads() < 2 {
        return;
    }
    unsafe { f.push_on_thread(7, 1) };
    assert_eq!(*f.as_ref()[1], vec![7]);
    assert_eq!(f.len(), 1);
}

#[test]
fn pop_drains_in_lifo_order_within_a_shard() {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    assert_eq!(f.pop(), Some(3));
    assert_eq!(f.pop(), Some(2));
    assert_eq!(f.pop(), Some(1));
    assert_eq!(f.pop(), None);
}

#[test]
fn pop_from_thread_drains_chosen_shard() {
    let f: Frontier<usize> = Frontier::from(vec![10, 20]);
    unsafe {
        assert_eq!(f.pop_from_thread(0), Some(20));
        assert_eq!(f.pop_from_thread(0), Some(10));
        assert_eq!(f.pop_from_thread(0), None);
    }
}

#[test]
fn is_empty_tracks_pushes() {
    let mut f: Frontier<usize> = Frontier::with_capacity(8);
    assert!(f.is_empty());
    f.push(1);
    assert!(!f.is_empty());
    f.clear();
    assert!(f.is_empty());
}

#[test]
fn shrink_to_fit_keeps_contents() {
    let mut f: Frontier<usize> = Frontier::with_capacity(1024);
    f.push(1);
    f.push(2);
    f.shrink_to_fit();
    assert_eq!(f.len(), 2);
    assert_eq!(f.iter().copied().collect::<Vec<_>>(), vec![1, 2]);
}

// --------------------------------------------------------------------------
// Conversions and trait impls
// --------------------------------------------------------------------------

#[test]
fn from_vec_lands_in_first_shard() {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    assert_eq!(*f.as_ref()[0], vec![1, 2, 3]);
    assert_eq!(f.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
}

#[test]
fn into_vec_of_vecs_round_trips_shards() {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2]);
    let n = f.number_of_threads();
    let vv: Vec<Vec<usize>> = f.into();
    assert_eq!(vv.len(), n);
    assert_eq!(vv[0], vec![1, 2]);
}

#[test]
fn into_flat_vec_uses_concat() {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].push(1);
    if f.number_of_threads() > 1 {
        f.as_mut()[1].push(2);
    }
    let v: Vec<usize> = f.into();
    assert!(v.iter().copied().any(|x| x == 1));
}

#[test]
fn concat_method_concatenates_shards() {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2]);
    if f.number_of_threads() > 1 {
        f.as_mut()[1].extend([3, 4]);
    }
    let c = f.concat();
    assert!(c.starts_with(&[1, 2]));
    assert_eq!(c.len(), f.len());
}

#[test]
fn clone_produces_equal_frontier() {
    let f: Frontier<usize> = Frontier::from(vec![5, 6, 7]);
    let g = f.clone();
    assert_eq!(f, g);
    // Mutating one does not affect the other.
    g.push(8);
    assert_ne!(f, g);
}

#[test]
fn debug_renders_struct_names() {
    let f: Frontier<usize> = Frontier::from(vec![1]);
    let s = format!("{:?}", f);
    assert!(s.contains("Frontier"));
    assert!(s.contains("threads"));
    let it = f.iter();
    let s = format!("{:?}", it);
    assert!(s.contains("FrontierIter"));
    assert!(s.contains("remaining"));
}

#[test]
fn partial_eq_distinguishes_lengths_and_contents() {
    let f: Frontier<usize> = Frontier::from(vec![1, 2]);
    let g: Frontier<usize> = Frontier::from(vec![1, 2]);
    let h: Frontier<usize> = Frontier::from(vec![1, 3]);
    let k: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    assert_eq!(f, g);
    assert_ne!(f, h);
    assert_ne!(f, k);
}

#[test]
fn as_mut_yields_mutable_shard_slice() {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].push(42);
    assert_eq!(f.len(), 1);
    assert_eq!(f.iter().copied().next(), Some(42));
}

// --------------------------------------------------------------------------
// Iterator views
// --------------------------------------------------------------------------

#[test]
fn vector_sizes_matches_iter_vectors() {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2]);
    let sizes = f.vector_sizes();
    let by_iter: Vec<usize> = f.iter_vectors().map(|v| v.len()).collect();
    assert_eq!(sizes, by_iter);
    assert_eq!(sizes[0], 2);
}

#[test]
fn iter_size_hint_and_count() {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3, 4, 5]);
    let it = f.iter();
    assert_eq!(it.size_hint(), (5, Some(5)));
    assert_eq!(f.iter().count(), 5);
}

#[test]
fn iter_is_empty_on_empty_frontier() {
    let f: Frontier<usize> = Frontier::new();
    let it = f.iter();
    assert!(it.is_empty());
    assert_eq!(it.len(), 0);
}

#[test]
fn iter_skips_empty_intermediate_shards() {
    let mut f: Frontier<usize> = Frontier::new();
    if f.number_of_threads() < 3 {
        return;
    }
    f.as_mut()[0].push(1);
    // shard 1 left empty
    f.as_mut()[2].push(3);
    let collected: Vec<usize> = f.iter().copied().collect();
    assert_eq!(collected, vec![1, 3]);
}

#[test]
fn iter_back_yields_reverse_within_shard() {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2, 3]);
    let collected: Vec<usize> = f.iter().rev().copied().collect();
    assert_eq!(collected, vec![3, 2, 1]);
}

#[test]
fn iter_back_crosses_shard_boundaries() {
    let mut f: Frontier<usize> = Frontier::new();
    if f.number_of_threads() < 2 {
        return;
    }
    f.as_mut()[0].extend([1, 2]);
    f.as_mut()[1].extend([3, 4]);
    let collected: Vec<usize> = f.iter().rev().copied().collect();
    assert_eq!(collected, vec![4, 3, 2, 1]);
}

#[test]
fn iter_back_skips_empty_trailing_shards() {
    let mut f: Frontier<usize> = Frontier::new();
    if f.number_of_threads() < 2 {
        return;
    }
    f.as_mut()[0].extend([1, 2]);
    // last shard left empty: next_back must walk back past it.
    let collected: Vec<usize> = f.iter().rev().copied().collect();
    assert_eq!(collected, vec![2, 1]);
}

// --------------------------------------------------------------------------
// Splitting (UnindexedProducer + Producer)
// --------------------------------------------------------------------------

#[test]
fn unindexed_split_below_threshold_returns_none() {
    let f: Frontier<usize> = Frontier::from(vec![]);
    let (this, other) = UnindexedProducer::split(FrontierProducer::new(&f));
    assert_eq!(this.len(), 0);
    assert!(other.is_none());

    let f: Frontier<usize> = Frontier::from(vec![42]);
    let (this, other) = UnindexedProducer::split(FrontierProducer::new(&f));
    assert_eq!(
        Producer::into_iter(this).copied().collect::<Vec<_>>(),
        vec![42]
    );
    assert!(other.is_none());
}

#[test]
fn unindexed_split_at_shard_boundary() {
    // Two equal shards make cumulative_lens.binary_search hit the Ok arm
    // (split happens exactly at a shard boundary).
    let mut f: Frontier<usize> = Frontier::new();
    if f.number_of_threads() < 2 {
        return;
    }
    f.as_mut()[0].extend([1, 2]);
    f.as_mut()[1].extend([3, 4]);
    let (low, high) = UnindexedProducer::split(FrontierProducer::new(&f));
    let low = Producer::into_iter(low);
    let high = Producer::into_iter(high.unwrap());
    assert_eq!(low.copied().collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(high.copied().collect::<Vec<_>>(), vec![3, 4]);
}

#[test]
fn unindexed_split_inside_shard() {
    // Single shard: split lands strictly inside, hitting the Err arm of locate.
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2, 3, 4]);
    let (low, high) = UnindexedProducer::split(FrontierProducer::new(&f));
    let low = Producer::into_iter(low);
    let high = Producer::into_iter(high.unwrap());
    assert_eq!(low.copied().collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(high.copied().collect::<Vec<_>>(), vec![3, 4]);
}

#[test]
fn unindexed_fold_with_consumes() {
    // `fold_with` is the other UnindexedProducer method; bridge_unindexed
    // exercises it via par_iter().
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2, 3]);
    if f.number_of_threads() > 1 {
        f.as_mut()[1].extend([4, 5]);
    }
    let total: usize = f.par_iter().copied().sum();
    assert_eq!(total, f.iter().copied().sum::<usize>());
}

#[test]
fn producer_split_at_boundary() {
    let mut f: Frontier<usize> = Frontier::new();
    if f.number_of_threads() < 2 {
        return;
    }
    f.as_mut()[0].extend([1, 2]);
    f.as_mut()[1].extend([3, 4]);
    let (lo, hi) = Producer::split_at(FrontierProducer::new(&f), 2);
    let lo = Producer::into_iter(lo);
    let hi = Producer::into_iter(hi);
    assert_eq!(lo.copied().collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(hi.copied().collect::<Vec<_>>(), vec![3, 4]);
}

#[test]
fn producer_split_at_inside_single_shard() {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2, 3, 4]);
    let (lo, hi) = Producer::split_at(FrontierProducer::new(&f), 1);
    let lo = Producer::into_iter(lo);
    let hi = Producer::into_iter(hi);
    assert_eq!(lo.copied().collect::<Vec<_>>(), vec![1]);
    assert_eq!(hi.copied().collect::<Vec<_>>(), vec![2, 3, 4]);
}

#[test]
fn producer_into_iter_walks_full_range() {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    let it = Producer::into_iter(FrontierProducer::new(&f));
    assert_eq!(it.copied().collect::<Vec<_>>(), vec![1, 2, 3]);
}

// --------------------------------------------------------------------------
// Parallel views
// --------------------------------------------------------------------------

#[test]
fn par_iter_opt_len_is_unindexed() {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    let pi = f.par_iter();
    // FrontierParIter is reported as unindexed via `ParallelIterator::opt_len`.
    assert!(<_ as ParallelIterator>::opt_len(&pi).is_none());
}

#[test]
fn par_iter_indexed_drive_collects() {
    // IndexedParallelIterator::drive invokes the producer machinery (split_at,
    // with_producer, etc.) — collect drives the indexed path.
    let mut f: Frontier<usize> = Frontier::new();
    for (i, v) in f.as_mut().iter_mut().enumerate() {
        v.extend(std::iter::repeat_n(i, 3));
    }
    let v: Vec<usize> = f.par_iter().copied().collect();
    assert_eq!(v.len(), 3 * f.number_of_threads());
}

#[test]
fn par_iter_indexed_len() {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    let pi = f.par_iter();
    assert_eq!(IndexedParallelIterator::len(&pi), 3);
}

#[test]
fn par_iter_vectors_yields_each_shard() {
    let mut f: Frontier<usize> = Frontier::new();
    for v in f.as_mut().iter_mut() {
        v.push(1);
    }
    let n = f.par_iter_vectors().count();
    assert_eq!(n, f.number_of_threads());
    let total: usize = f.par_iter_vectors().map(|v| v.len()).sum();
    assert_eq!(total, f.number_of_threads());
}

#[test]
fn par_iter_vectors_mut_appends_to_each() {
    let mut f: Frontier<usize> = Frontier::new();
    for v in f.as_mut().iter_mut() {
        v.push(1);
    }
    f.par_iter_vectors_mut().for_each(|v| v.push(2));
    assert_eq!(f.len(), 2 * f.number_of_threads());
}

#[test]
fn into_par_iter_vectors_consumes() {
    let mut f: Frontier<usize> = Frontier::new();
    for v in f.as_mut().iter_mut() {
        v.push(7);
    }
    let n = f.number_of_threads();
    let total: usize = f.into_par_iter_vectors().map(|v| v.len()).sum();
    assert_eq!(total, n);
}

// next_back when the iterator was already exhausted from the front (forces
// the second early-return branch in next_back).
#[test]
fn next_back_after_forward_exhaustion_returns_none() {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    let mut it = f.iter();
    while it.next().is_some() {}
    assert_eq!(it.next_back(), None);
}

// `IndexedParallelIterator::drive` (as opposed to `drive_unindexed`) is hit
// when the consumer is an indexed one. `collect_into_vec` is the canonical
// indexed-only sink.
#[test]
fn par_iter_indexed_drive_via_collect_into_vec() {
    let mut f: Frontier<usize> = Frontier::new();
    for v in f.as_mut().iter_mut() {
        v.extend([1, 2, 3]);
    }
    let mut out: Vec<&usize> = Vec::new();
    IndexedParallelIterator::collect_into_vec(f.par_iter(), &mut out);
    assert_eq!(out.len(), 3 * f.number_of_threads());
}

#[test]
fn frontier_producer_debug_renders_fields() {
    let f: Frontier<usize> = Frontier::from(vec![1, 2]);
    let s = format!("{:?}", FrontierProducer::new(&f));
    assert!(s.contains("FrontierProducer"));
    assert!(s.contains("start"));
    assert!(s.contains("end"));
    assert!(s.contains("cumulative_lens"));
}

#[test]
fn frontier_producer_is_empty_tracks_range() {
    let f: Frontier<usize> = Frontier::new();
    assert!(FrontierProducer::new(&f).is_empty());
    let f: Frontier<usize> = Frontier::from(vec![1]);
    assert!(!FrontierProducer::new(&f).is_empty());
}

#[test]
fn extend_round_robins_values_across_shards() {
    let mut f: Frontier<usize> = Frontier::new();
    let n = f.number_of_threads();
    // Use a length that is a clean multiple of `n` so every shard gets the
    // same count regardless of the system thread count.
    let total = n * 4;
    f.extend(0..total);
    assert_eq!(f.len(), total);
    let sizes = f.vector_sizes();
    for size in &sizes {
        assert_eq!(*size, 4);
    }
    // Round-robin: shard `i` receives values `i, i+n, i+2n, ...`.
    for (shard_idx, shard) in f.iter_vectors().enumerate() {
        let expected: Vec<usize> = (0..4).map(|k| shard_idx + k * n).collect();
        assert_eq!(shard, &expected);
    }
}

#[test]
fn extend_appends_to_existing_contents() {
    let mut f: Frontier<usize> = Frontier::new();
    let n = f.number_of_threads();
    f.extend(0..n); // one element per shard
    f.extend(n..2 * n); // another element per shard
    assert_eq!(f.len(), 2 * n);
    for (shard_idx, shard) in f.iter_vectors().enumerate() {
        assert_eq!(shard, &vec![shard_idx, shard_idx + n]);
    }
}

#[test]
fn extend_handles_unknown_size_iterator() {
    let mut f: Frontier<usize> = Frontier::new();
    let n = f.number_of_threads();
    // `filter` erases the size hint, so the reserve branch is skipped.
    f.extend((0..n * 2).filter(|_| true));
    assert_eq!(f.len(), n * 2);
}
