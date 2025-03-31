# Parallel Frontier

[![downloads](https://img.shields.io/crates/d/parallel_frontier)](https://crates.io/crates/parallel_frontier)
[![dependents](https://img.shields.io/librariesio/dependents/cargo/parallel_frontier)](https://crates.io/crates/parallel_frontier/reverse_dependencies)
![GitHub CI](https://github.com/zommiommy/parallel_frontier/actions/workflows/rust.yml/badge.svg)
![license](https://img.shields.io/crates/l/parallel_frontier)
[![Line count](https://tokei.rs/b1/github/zommiommy/parallel_frontier)](https://github.com/zommiommy/parallel_frontier)
[![Latest version](https://img.shields.io/crates/v/parallel_frontier.svg)](https://crates.io/crates/parallel_frontier)
[![Documentation](https://docs.rs/parallel_frontier/badge.svg)](https://docs.rs/parallel_frontier)

A queue-like frontier for breath-first visits on graphs that supports
constant-time concurrent pushes and parallel iteration.

Iteration order is not guaranteed to be the same as the order of insertion,
contrarily to a classical FIFO queue, but preserving the order of insertion is
not necessary for the correctness of breadth-first visits as long as visits are
performed in rounds associated with increasing distances.

Pushes are per-thread, and require no synchronization: each thread has its
separate *shard* where elements are enqueued. Parallel iteration is performed by
merging virtually the shards, without any copying. Iteration can be sequential
or parallel (using Rayon's [`ParallelIterator`]).

Do note that while the overall order in which elements are pushed is not
preserved, the order in each shard is. This means that if you push the same
elements in the same order in each thread, the resulting sequential iterator
will yield the elements in the same order. The same guarantee is not possible in
the parallel case because we depend on Rayon's [`ParallelIterator`] behavior.

## Why

The goal of this structure is to do faster parallel breath-first search. Since
each thread will work only on its shard, there is no need for any concurrency
mechanism. The virtual merging of the shards avoids copying.

## Examples

The following example shows how to use the `Frontier` structure to perform a
breadth-first visit on a graph. The part representing access to the graph
have been omitted:

```ignore
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

[`ParallelIterator`]: <https://docs.rs/rayon/latest/rayon/iter/trait.ParallelIterator.html>
