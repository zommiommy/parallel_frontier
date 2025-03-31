# Parallel Frontier
=======
[![downloads](https://img.shields.io/crates/d/parallel_frontier)](https://crates.io/crates/parallel_frontier)
[![dependents](https://img.shields.io/librariesio/dependents/cargo/parallel_frontier)](https://crates.io/crates/parallel_frontier/reverse_dependencies)
![GitHub CI](https://github.com/zommiommy/parallel_frontier/actions/workflows/rust.yml/badge.svg)
![license](https://img.shields.io/crates/l/parallel_frontier)
[![Line count](https://tokei.rs/b1/github/zommiommy/parallel_frontier)](https://github.com/zommiommy/parallel_frontier)
[![Latest version](https://img.shields.io/crates/v/parallel_frontier.svg)](https://crates.io/crates/parallel_frontier)
[![Documentation](https://docs.rs/parallel_frontier/badge.svg)](https://docs.rs/parallel_frontier)
[![Coverage Status](https://coveralls.io/repos/github/zommiommy/parallel_frontier/badge.svg?branch=main)](https://coveralls.io/github/zommiommy/parallel_frontier?branch=main)

Unordered vector of elements with constant time concurrent push.

## What this is

This struct should be used when you intend to create a vector in parallel but you
do not have the constraint of having the elements in this vector sorted by the
order they have been inserted.

This struct avoids to copy resulting shards into a single large one,
as it is done for instance in the [Rayon](https://docs.rs/rayon/latest/rayon/)'s `collect::<Vec<_>>()`
operation.

Once the frontier has been populated, it is possible to iterate all the elements in it,
both sequentially and in parallel (using Rayon's [`ParallelIterator`](https://docs.rs/rayon/latest/rayon/iter/trait.ParallelIterator.html)). 
Do note that while the elements are NOT sorted, the sequential iterator
will always maintain a consistent order of the elements,

## What this is NOT
This is NOT a set: the elements in the frontier are NOT necessarily unique and
the lack of order does not imply that this is handled in any way such a set.

## Why

The goal of this structure is to do faster parallel breath-first search.
Since each thread will work only on its version of the vec, there is no need
for any concurrency mechanism.

```rust
use parallel_frontier::prelude::{Frontier, ParallelIterator};
use rayon::{prelude::*, ThreadPool};

fn par_bfs(roots: &[usize], thread_pool: ThreadPool) {
    let mut curr_frontier = Frontier::with_threads(thread_pool, None);
    let mut next_frontier = Frontier::with_threads(thread_pool, None);
    /// add the roots to the first 
    curr_frontier.as_mut()[0].extend(roots);
    let num_nodes = todo!(); // assume you know the number of nodes in the graph
    let mut visited = vec![false; num_nodes];

    while !curr_frontier.is_empty() {
        curr_frontier.par_iter().for_each(|node| {
            // visit the node
            for succ in todo!() { // get the successors with your favorite graph impl
                if !visited[succ] {
                    visited[succ] = true;
                    // add it to the next_frontier, this implicitly uses
                    // rayon thread_index to push without locks and contentions
                    next_frontier.push(succ);
                }
            }
        });
        // swap curr and next frontier
        std::mem::swap(&mut curr_frontier, &mut next_frontier);
        // clean it up for the next iteration
        next_frontier.clear();
    }
}
```