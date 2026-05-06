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
fn test_default_matches_new() -> anyhow::Result<()> {
    let a: Frontier<usize> = Frontier::default();
    let b: Frontier<usize> = Frontier::new();
    assert_eq!(a, b);
    assert_eq!(a.number_of_threads(), b.number_of_threads());
    Ok(())
}

#[test]
fn test_with_capacity_creates_empty_frontier() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::with_capacity(64);
    assert!(f.is_empty());
    assert!(f.number_of_threads() >= 1);
    Ok(())
}

#[test]
fn test_with_threads_capacity_uses_pool_size() -> anyhow::Result<()> {
    let pool = ThreadPoolBuilder::default().num_threads(2).build()?;
    let f: Frontier<usize> = Frontier::with_threads(&pool, Some(64));
    assert_eq!(f.number_of_threads(), 2);
    assert!(f.is_empty());
    Ok(())
}

// --------------------------------------------------------------------------
// Push / pop / state
// --------------------------------------------------------------------------

#[test]
fn test_push_outside_pool_targets_shard_zero() -> anyhow::Result<()> {
    // self.threads = Some(pool) but call site is the main thread (no rayon
    // pool active), exercising the `0` branch of get_current_thread_index.
    let pool = ThreadPoolBuilder::default().num_threads(2).build()?;
    let f: Frontier<usize> = Frontier::with_threads(&pool, None);
    f.push(1);
    f.push(2);
    assert_eq!(*f.as_ref()[0], vec![1, 2]);
    assert!(f.as_ref()[1].is_empty());
    Ok(())
}

#[test]
fn test_push_inside_assigned_pool_uses_thread_index() -> anyhow::Result<()> {
    let pool = ThreadPoolBuilder::default().num_threads(2).build()?;
    let f: Frontier<usize> = Frontier::with_threads(&pool, None);
    pool.install(|| {
        (0..200).into_par_iter().for_each(|i| f.push(i));
    });
    assert_eq!(f.len(), 200);
    Ok(())
}

#[test]
#[should_panic(expected = "Parallel frontier called from external thread pool")]
fn test_push_from_foreign_pool_panics() {
    let inner = ThreadPoolBuilder::default()
        .num_threads(1)
        .build()
        .expect("inner pool builds");
    let outer = ThreadPoolBuilder::default()
        .num_threads(1)
        .build()
        .expect("outer pool builds");
    let f: Frontier<usize> = Frontier::with_threads(&inner, None);
    outer.install(|| f.push(0));
}

#[test]
fn test_push_on_thread_targets_chosen_shard() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::new();
    if f.number_of_threads() < 2 {
        return Ok(());
    }
    unsafe { f.push_on_thread(7, 1) };
    assert_eq!(*f.as_ref()[1], vec![7]);
    assert_eq!(f.len(), 1);
    Ok(())
}

#[test]
fn test_pop_drains_in_lifo_order_within_a_shard() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    assert_eq!(f.pop(), Some(3));
    assert_eq!(f.pop(), Some(2));
    assert_eq!(f.pop(), Some(1));
    assert_eq!(f.pop(), None);
    Ok(())
}

#[test]
fn test_pop_from_thread_drains_chosen_shard() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![10, 20]);
    unsafe {
        assert_eq!(f.pop_from_thread(0), Some(20));
        assert_eq!(f.pop_from_thread(0), Some(10));
        assert_eq!(f.pop_from_thread(0), None);
    }
    Ok(())
}

#[test]
fn test_is_empty_tracks_pushes() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::with_capacity(8);
    assert!(f.is_empty());
    f.push(1);
    assert!(!f.is_empty());
    f.clear();
    assert!(f.is_empty());
    Ok(())
}

#[test]
fn test_shrink_to_fit_keeps_contents() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::with_capacity(1024);
    f.push(1);
    f.push(2);
    f.shrink_to_fit();
    assert_eq!(f.len(), 2);
    assert_eq!(f.iter().copied().collect::<Vec<_>>(), vec![1, 2]);
    Ok(())
}

// --------------------------------------------------------------------------
// Conversions and trait impls
// --------------------------------------------------------------------------

#[test]
fn test_from_vec_lands_in_first_shard() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    assert_eq!(*f.as_ref()[0], vec![1, 2, 3]);
    assert_eq!(f.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
    Ok(())
}

#[test]
fn test_into_vec_of_vecs_round_trips_shards() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2]);
    let n = f.number_of_threads();
    let vv: Vec<Vec<usize>> = f.into();
    assert_eq!(vv.len(), n);
    assert_eq!(vv[0], vec![1, 2]);
    Ok(())
}

#[test]
fn test_into_flat_vec_uses_concat() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].push(1);
    if f.number_of_threads() > 1 {
        f.as_mut()[1].push(2);
    }
    let v: Vec<usize> = f.into();
    assert!(v.iter().copied().any(|x| x == 1));
    Ok(())
}

#[test]
fn test_concat_method_concatenates_shards() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2]);
    if f.number_of_threads() > 1 {
        f.as_mut()[1].extend([3, 4]);
    }
    let c = f.concat();
    assert!(c.starts_with(&[1, 2]));
    assert_eq!(c.len(), f.len());
    Ok(())
}

#[test]
fn test_clone_produces_equal_frontier() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![5, 6, 7]);
    let g = f.clone();
    assert_eq!(f, g);
    // Mutating one does not affect the other.
    g.push(8);
    assert_ne!(f, g);
    Ok(())
}

#[test]
fn test_debug_renders_struct_names() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![1]);
    let s = format!("{:?}", f);
    assert!(s.contains("Frontier"));
    assert!(s.contains("threads"));
    let it = f.iter();
    let s = format!("{:?}", it);
    assert!(s.contains("FrontierIter"));
    assert!(s.contains("remaining"));
    Ok(())
}

#[test]
fn test_partial_eq_distinguishes_lengths_and_contents() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![1, 2]);
    let g: Frontier<usize> = Frontier::from(vec![1, 2]);
    let h: Frontier<usize> = Frontier::from(vec![1, 3]);
    let k: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    assert_eq!(f, g);
    assert_ne!(f, h);
    assert_ne!(f, k);
    Ok(())
}

#[test]
fn test_as_mut_yields_mutable_shard_slice() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].push(42);
    assert_eq!(f.len(), 1);
    assert_eq!(f.iter().copied().next(), Some(42));
    Ok(())
}

#[test]
fn test_extend_funnels_values_into_first_shard() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    let n = f.number_of_threads();
    let total = n * 4;
    f.extend(0..total);
    assert_eq!(f.len(), total);
    let sizes = f.vector_sizes();
    // All values are pushed to shard 0; the rest stay empty.
    assert_eq!(sizes[0], total);
    for size in &sizes[1..] {
        assert_eq!(*size, 0);
    }
    let collected: Vec<usize> = f.iter().copied().collect();
    let expected: Vec<usize> = (0..total).collect();
    assert_eq!(collected, expected);
    Ok(())
}

#[test]
fn test_extend_appends_to_existing_contents() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    let n = f.number_of_threads();
    f.extend(0..n);
    f.extend(n..2 * n);
    assert_eq!(f.len(), 2 * n);
    let sizes = f.vector_sizes();
    assert_eq!(sizes[0], 2 * n);
    for size in &sizes[1..] {
        assert_eq!(*size, 0);
    }
    let collected: Vec<usize> = f.iter().copied().collect();
    let expected: Vec<usize> = (0..2 * n).collect();
    assert_eq!(collected, expected);
    Ok(())
}

#[test]
fn test_extend_handles_unknown_size_iterator() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    let n = f.number_of_threads();
    // `filter` erases the size hint, so the reserve branch is skipped.
    f.extend((0..n * 2).filter(|_| true));
    assert_eq!(f.len(), n * 2);
    Ok(())
}

// --------------------------------------------------------------------------
// Iterator views
// --------------------------------------------------------------------------

#[test]
fn test_vector_sizes_matches_iter_vectors() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2]);
    let sizes = f.vector_sizes();
    let by_iter: Vec<usize> = f.iter_vectors().map(|v| v.len()).collect();
    assert_eq!(sizes, by_iter);
    assert_eq!(sizes[0], 2);
    Ok(())
}

#[test]
fn test_iter_size_hint_and_count() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3, 4, 5]);
    let it = f.iter();
    assert_eq!(it.size_hint(), (5, Some(5)));
    assert_eq!(f.iter().count(), 5);
    Ok(())
}

#[test]
fn test_iter_is_empty_on_empty_frontier() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::new();
    let it = f.iter();
    assert!(it.is_empty());
    assert_eq!(it.len(), 0);
    Ok(())
}

#[test]
fn test_iter_skips_empty_intermediate_shards() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    if f.number_of_threads() < 3 {
        return Ok(());
    }
    f.as_mut()[0].push(1);
    // shard 1 left empty
    f.as_mut()[2].push(3);
    let collected: Vec<usize> = f.iter().copied().collect();
    assert_eq!(collected, vec![1, 3]);
    Ok(())
}

#[test]
fn test_iter_back_yields_reverse_within_shard() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2, 3]);
    let collected: Vec<usize> = f.iter().rev().copied().collect();
    assert_eq!(collected, vec![3, 2, 1]);
    Ok(())
}

#[test]
fn test_iter_back_crosses_shard_boundaries() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    if f.number_of_threads() < 2 {
        return Ok(());
    }
    f.as_mut()[0].extend([1, 2]);
    f.as_mut()[1].extend([3, 4]);
    let collected: Vec<usize> = f.iter().rev().copied().collect();
    assert_eq!(collected, vec![4, 3, 2, 1]);
    Ok(())
}

#[test]
fn test_iter_back_skips_empty_trailing_shards() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    if f.number_of_threads() < 2 {
        return Ok(());
    }
    f.as_mut()[0].extend([1, 2]);
    // last shard left empty: next_back must walk back past it.
    let collected: Vec<usize> = f.iter().rev().copied().collect();
    assert_eq!(collected, vec![2, 1]);
    Ok(())
}

// --------------------------------------------------------------------------
// Splitting (UnindexedProducer + Producer)
// --------------------------------------------------------------------------

#[test]
fn test_unindexed_split_below_threshold_returns_none() -> anyhow::Result<()> {
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
    Ok(())
}

#[test]
fn test_unindexed_split_at_shard_boundary() -> anyhow::Result<()> {
    // Two equal shards make cumulative_lens.binary_search hit the Ok arm
    // (split happens exactly at a shard boundary).
    let mut f: Frontier<usize> = Frontier::new();
    if f.number_of_threads() < 2 {
        return Ok(());
    }
    f.as_mut()[0].extend([1, 2]);
    f.as_mut()[1].extend([3, 4]);
    let (low, high) = UnindexedProducer::split(FrontierProducer::new(&f));
    let low = Producer::into_iter(low);
    let high = Producer::into_iter(high.unwrap());
    assert_eq!(low.copied().collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(high.copied().collect::<Vec<_>>(), vec![3, 4]);
    Ok(())
}

#[test]
fn test_unindexed_split_inside_shard() -> anyhow::Result<()> {
    // Single shard: split lands strictly inside, hitting the Err arm of locate.
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2, 3, 4]);
    let (low, high) = UnindexedProducer::split(FrontierProducer::new(&f));
    let low = Producer::into_iter(low);
    let high = Producer::into_iter(high.unwrap());
    assert_eq!(low.copied().collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(high.copied().collect::<Vec<_>>(), vec![3, 4]);
    Ok(())
}

#[test]
fn test_unindexed_fold_with_consumes() -> anyhow::Result<()> {
    // `fold_with` is the other UnindexedProducer method; bridge_unindexed
    // exercises it via par_iter().
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2, 3]);
    if f.number_of_threads() > 1 {
        f.as_mut()[1].extend([4, 5]);
    }
    let total: usize = f.par_iter().copied().sum();
    assert_eq!(total, f.iter().copied().sum::<usize>());
    Ok(())
}

#[test]
fn test_producer_split_at_boundary() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    if f.number_of_threads() < 2 {
        return Ok(());
    }
    f.as_mut()[0].extend([1, 2]);
    f.as_mut()[1].extend([3, 4]);
    let (lo, hi) = Producer::split_at(FrontierProducer::new(&f), 2);
    let lo = Producer::into_iter(lo);
    let hi = Producer::into_iter(hi);
    assert_eq!(lo.copied().collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(hi.copied().collect::<Vec<_>>(), vec![3, 4]);
    Ok(())
}

#[test]
fn test_producer_split_at_inside_single_shard() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    f.as_mut()[0].extend([1, 2, 3, 4]);
    let (lo, hi) = Producer::split_at(FrontierProducer::new(&f), 1);
    let lo = Producer::into_iter(lo);
    let hi = Producer::into_iter(hi);
    assert_eq!(lo.copied().collect::<Vec<_>>(), vec![1]);
    assert_eq!(hi.copied().collect::<Vec<_>>(), vec![2, 3, 4]);
    Ok(())
}

#[test]
fn test_producer_into_iter_walks_full_range() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    let it = Producer::into_iter(FrontierProducer::new(&f));
    assert_eq!(it.copied().collect::<Vec<_>>(), vec![1, 2, 3]);
    Ok(())
}

// --------------------------------------------------------------------------
// Parallel views
// --------------------------------------------------------------------------

#[test]
fn test_par_iter_opt_len_is_unindexed() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    let pi = f.par_iter();
    // FrontierParIter is reported as unindexed via `ParallelIterator::opt_len`.
    assert!(<_ as ParallelIterator>::opt_len(&pi).is_none());
    Ok(())
}

#[test]
fn test_par_iter_indexed_drive_collects() -> anyhow::Result<()> {
    // IndexedParallelIterator::drive invokes the producer machinery (split_at,
    // with_producer, etc.) — collect drives the indexed path.
    let mut f: Frontier<usize> = Frontier::new();
    for (i, v) in f.as_mut().iter_mut().enumerate() {
        v.extend(std::iter::repeat_n(i, 3));
    }
    let v: Vec<usize> = f.par_iter().copied().collect();
    assert_eq!(v.len(), 3 * f.number_of_threads());
    Ok(())
}

#[test]
fn test_par_iter_indexed_len() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    let pi = f.par_iter();
    assert_eq!(IndexedParallelIterator::len(&pi), 3);
    Ok(())
}

#[test]
fn test_par_iter_vectors_yields_each_shard() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    for v in f.as_mut().iter_mut() {
        v.push(1);
    }
    let n = f.par_iter_vectors().count();
    assert_eq!(n, f.number_of_threads());
    let total: usize = f.par_iter_vectors().map(|v| v.len()).sum();
    assert_eq!(total, f.number_of_threads());
    Ok(())
}

#[test]
fn test_par_iter_vectors_mut_appends_to_each() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    for v in f.as_mut().iter_mut() {
        v.push(1);
    }
    f.par_iter_vectors_mut().for_each(|v| v.push(2));
    assert_eq!(f.len(), 2 * f.number_of_threads());
    Ok(())
}

#[test]
fn test_into_par_iter_vectors_consumes() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    for v in f.as_mut().iter_mut() {
        v.push(7);
    }
    let n = f.number_of_threads();
    let total: usize = f.into_par_iter_vectors().map(|v| v.len()).sum();
    assert_eq!(total, n);
    Ok(())
}

// next_back when the iterator was already exhausted from the front (forces
// the second early-return branch in next_back).
#[test]
fn test_next_back_after_forward_exhaustion_returns_none() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![1, 2, 3]);
    let mut it = f.iter();
    while it.next().is_some() {}
    assert_eq!(it.next_back(), None);
    Ok(())
}

// `IndexedParallelIterator::drive` (as opposed to `drive_unindexed`) is hit
// when the consumer is an indexed one. `collect_into_vec` is the canonical
// indexed-only sink.
#[test]
fn test_par_iter_indexed_drive_via_collect_into_vec() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    for v in f.as_mut().iter_mut() {
        v.extend([1, 2, 3]);
    }
    let mut out: Vec<&usize> = Vec::new();
    IndexedParallelIterator::collect_into_vec(f.par_iter(), &mut out);
    assert_eq!(out.len(), 3 * f.number_of_threads());
    Ok(())
}

#[test]
fn test_frontier_producer_debug_renders_fields() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![1, 2]);
    let s = format!("{:?}", FrontierProducer::new(&f));
    assert!(s.contains("FrontierProducer"));
    assert!(s.contains("start"));
    assert!(s.contains("end"));
    assert!(s.contains("cumulative_lens"));
    Ok(())
}

#[test]
fn test_frontier_producer_is_empty_tracks_range() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::new();
    assert!(FrontierProducer::new(&f).is_empty());
    let f: Frontier<usize> = Frontier::from(vec![1]);
    assert!(!FrontierProducer::new(&f).is_empty());
    Ok(())
}

#[test]
fn test_iter_is_fused() -> anyhow::Result<()> {
    let f: Frontier<usize> = Frontier::from(vec![1]);
    let mut it = f.iter();
    assert_eq!(it.next(), Some(&1));
    assert_eq!(it.next(), None);
    // After exhaustion the iterator stays empty (FusedIterator contract).
    assert_eq!(it.next(), None);
    assert_eq!(it.next_back(), None);
    Ok(())
}

#[test]
fn test_par_clear_empties_every_shard() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::new();
    for v in f.as_mut().iter_mut() {
        v.extend([1, 2, 3]);
    }
    assert_eq!(f.len(), 3 * f.number_of_threads());
    f.par_clear();
    assert!(f.is_empty());
    Ok(())
}

#[test]
fn test_par_shrink_to_fit_keeps_contents() -> anyhow::Result<()> {
    let mut f: Frontier<usize> = Frontier::with_capacity(1024);
    f.push(1);
    f.push(2);
    f.par_shrink_to_fit();
    assert_eq!(f.len(), 2);
    assert_eq!(f.iter().copied().collect::<Vec<_>>(), vec![1, 2]);
    Ok(())
}
