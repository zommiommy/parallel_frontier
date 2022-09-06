extern crate parallel_frontier;
use parallel_frontier::*;
use rayon::iter::plumbing::UnindexedProducer;
use rayon::prelude::*;

#[test]
fn test_par_iter() {
    let frontier = Frontier::new();
    let vals: Vec<usize> = (0..10).collect::<Vec<_>>();

    for i in &vals {
        frontier.push(*i);
    }

    assert_eq!(vals, frontier.iter().copied().collect::<Vec<_>>());

    let (low, high) = frontier.iter().split();
    let high = high.unwrap();
    assert_eq!(
        (low.size_hint(), high.size_hint()),
        ((5, Some(5)), (5, Some(5)))
    );
    assert_eq!((low.count(), high.count()), (5, 5));
    assert_eq!(
        vals.iter().copied().sum::<usize>(),
        frontier.par_iter().copied().sum()
    );
    assert_eq!(vals, frontier.par_iter().copied().collect::<Vec<_>>());
}

#[test]
fn test_par_iter_with_par_push() {
    let frontier = Frontier::new();

    let m = 24;
    let n = 1000;

    (0..m).into_par_iter().for_each(|_| {
        for i in 0..n {
            frontier.push(i);
        }
    });

    println!("{:?}", frontier.vector_sizes());
    assert_eq!(m * n, frontier.par_iter().copied().count());
}

#[test]
fn test_sorted_frontiers() {
    let frontier = Frontier::new();

    let m = 24;
    let n = 1000;

    (0..m).into_par_iter().for_each(|_| {
        for i in 0..n {
            frontier.push(i);
        }
    });

    let new_frontier: Frontier<i32> = frontier
        .par_iter_vectors()
        .map(|x| x.clone())
        .collect::<Vec<Vec<_>>>()
        .try_into()
        .unwrap();

    assert_eq!(new_frontier, frontier);
}

#[test]
fn test_enumerate() {
    let frontier = Frontier::new();

    let m = 24;
    let n = 1000;

    (0..m).into_par_iter().for_each(|_| {
        for i in 0..n {
            frontier.push(i);
        }
    });

    println!("{:?}", frontier.vector_sizes());
    assert_eq!(m * n, frontier.par_iter().enumerate().count());
}