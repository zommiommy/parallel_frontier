/*
 * SPDX-FileCopyrightText: 2025 Tommaso Fontana
 * SPDX-FileCopyrightText: 2025 Luca Cappelletti
 *
 * SPDX-License-Identifier: Apache-2.0 OR LGPL-2.1-or-later
 */

extern crate parallel_frontier;
use parallel_frontier::*;
use rayon::{
    iter::plumbing::{Producer, UnindexedProducer},
    prelude::*,
    ThreadPoolBuilder,
};

#[test]
fn test_par_iter() -> anyhow::Result<()> {
    let frontier = Frontier::new();
    let vals: Vec<usize> = (0..10).collect::<Vec<_>>();

    for i in &vals {
        frontier.push(*i);
    }

    assert_eq!(frontier.iter().copied().collect::<Vec<_>>(), vals,);

    let (low, high) = UnindexedProducer::split(FrontierProducer::new(&frontier));
    let low = Producer::into_iter(low);
    let high = Producer::into_iter(high.unwrap());
    assert_eq!(
        (low.size_hint(), high.size_hint()),
        ((5, Some(5)), (5, Some(5)))
    );
    assert_eq!((low.count(), high.count()), (5, 5));
    assert_eq!(
        frontier.par_iter().copied().sum::<usize>(),
        vals.iter().copied().sum::<usize>(),
    );
    assert_eq!(frontier.par_iter().copied().collect::<Vec<_>>(), vals,);
    Ok(())
}

#[test]
fn test_par_iter_with_par_push() -> anyhow::Result<()> {
    let frontier = Frontier::new();

    let m = 24;
    let n = 1000;

    (0..m).into_par_iter().for_each(|_| {
        for i in 0..n {
            frontier.push(i);
        }
    });

    println!("{:?}", frontier.vector_sizes());
    assert_eq!(frontier.par_iter().copied().count(), m * n);
    Ok(())
}

#[test]
fn test_par_iter_with_par_push_with_thread_pool() -> anyhow::Result<()> {
    let pool = ThreadPoolBuilder::default().num_threads(3).build()?;
    let frontier = Frontier::with_threads(&pool, None);

    let m = 24;
    let n = 1000;

    pool.install(|| {
        (0..m).into_par_iter().for_each(|_| {
            for i in 0..n {
                frontier.push(i);
            }
        });
    });

    println!("{:?}", frontier.vector_sizes());
    assert_eq!(frontier.par_iter().copied().count(), m * n);
    assert_eq!(frontier.number_of_threads(), 3);
    Ok(())
}

#[test]
fn test_enumerate() -> anyhow::Result<()> {
    let frontier = Frontier::new();

    let m = 24;
    let n = 1000;

    (0..m).into_par_iter().for_each(|_| {
        for i in 0..n {
            frontier.push(i);
        }
    });

    println!("{:?}", frontier.vector_sizes());
    assert_eq!(frontier.par_iter().enumerate().count(), m * n);
    Ok(())
}
