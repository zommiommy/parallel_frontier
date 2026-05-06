# Parallel Frontier

[![downloads](https://img.shields.io/crates/d/parallel_frontier)](https://crates.io/crates/parallel_frontier)
[![dependents](https://img.shields.io/librariesio/dependents/cargo/parallel_frontier)](https://crates.io/crates/parallel_frontier/reverse_dependencies)
![GitHub CI](https://github.com/zommiommy/parallel_frontier/actions/workflows/rust.yml/badge.svg)
![license](https://img.shields.io/crates/l/parallel_frontier)
[![Line count](https://tokei.rs/b1/github/zommiommy/parallel_frontier)](https://github.com/zommiommy/parallel_frontier)
[![Latest version](https://img.shields.io/crates/v/parallel_frontier.svg)](https://crates.io/crates/parallel_frontier)
[![Documentation](https://docs.rs/parallel_frontier/badge.svg)](https://docs.rs/parallel_frontier)

A queue-like data structure for breadth-first graph visits with **lock-free
concurrent pushes** and **parallel iteration**. Each Rayon worker writes to its
own per-thread shard; the shards are then virtually merged for iteration
without any copy.

Iteration order is not the order of insertion - that is irrelevant for BFS
as long as visits proceed in rounds of increasing distance - but the order
within each shard is preserved.

## Highlights

- **Lock-free pushes.** Each thread targets its own [`Shard<T>`][shard] via
  [`rayon::current_thread_index`][rayon-tindex]; no synchronization on the
  hot path.
- **Cache-line padded shards.** [`Shard<T>`][shard] is `#[repr(align(64))]`,
  so per-thread `Vec` headers sit on distinct cache lines and concurrent
  pushes do not false-share the `len` field.
- **Zero-copy iteration.** Both [`Frontier::iter`][iter-fn] (sequential) and
  [`Frontier::par_iter`][par-iter-fn] (Rayon parallel) walk every shard as
  one flat sequence; splits are O(1) range arithmetic.
- **Safe initialization.** [`Frontier`][frontier] implements
  [`Extend<T>`][std-extend], round-robin across shards, so seeding the
  frontier from any iterator is one safe call under `&mut self`.
- **`FusedIterator`, `ExactSizeIterator`, `DoubleEndedIterator`** on the
  sequential iterator.

## Quickstart

A parallel breadth-first visit. The visited bitmap needs atomic updates
because every Rayon worker writes to it concurrently; this example assumes
something like [`sux::bits::AtomicBitVec`][sux-atomic].

```ignore
use parallel_frontier::Frontier;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;

fn par_bfs(graph: &impl Graph, roots: &[usize]) {
    let pool = ThreadPoolBuilder::new().build().unwrap();
    let mut curr = Frontier::with_threads(&pool, None);
    let mut next = Frontier::with_threads(&pool, None);

    // Distribute roots across shards before the first round.
    curr.extend(roots.iter().copied());
    let visited = AtomicBitVec::new(graph.num_nodes());
    for &r in roots {
        visited.set(r, true);
    }

    pool.install(|| {
        while !curr.is_empty() {
            curr.par_iter().for_each(|&node| {
                for succ in graph.successors(node) {
                    // `swap` returns the previous value, so this both
                    // marks the node visited and tells us whether we
                    // were the first to do so.
                    if !visited.swap(succ, true) {
                        // Pushes onto this Rayon worker's own shard.
                        // No locking, no contention.
                        next.push(succ);
                    }
                }
            });
            std::mem::swap(&mut curr, &mut next);
            next.clear();
        }
    });
}
```

For finer-grained control over the per-shard push path, see
[`Frontier::push_on_thread`][push-on-thread] (skips the per-call thread-local
lookup) and [`AsRef<[Shard<T>]>`][as-ref] / [`AsMut<[Shard<T>]>`][as-mut]
(direct slice access to the shards).

## Design

A [`Frontier<'a, T>`][frontier] owns one [`Shard<T>`][shard] per Rayon
worker. `Shard<T>` is a `repr(align(64))` newtype over
`UnsafeCell<Vec<T>>`: the alignment puts each shard's `Vec` header on its
own cache line, and the `UnsafeCell` lets [`push`][push] take `&self`
without any synchronization. The soundness contract is that each worker
mutates only its own shard, which is enforced by deriving the shard index
from [`rayon::current_thread_index`][rayon-tindex].

Parallel iteration uses [`FrontierProducer`][producer] as the Rayon
`Producer` / `UnindexedProducer`. The producer pre-computes a cumulative
shard-length table once (stored as an `Arc<[usize]>` shared across splits),
then splits via pure range arithmetic on the half-open interval
`[start, end)` over the conceptually-flattened sequence. The
`(shard_idx, offset)` lookup happens lazily, only on the leaves, in
`Producer::into_iter`.

## Testing

Standard test run:

```sh
cargo test --all-features --all-targets
```

### Miri

The crate is verified UB-free under Miri's [Tree Borrows][tb] aliasing
model:

```sh
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-disable-isolation -Zmiri-ignore-leaks" \
    cargo +nightly miri test
```

Flag rationale:

- `-Zmiri-tree-borrows` - de-facto standard aliasing model for crates that
  pull in Rayon. Stacked Borrows trips on a known false positive inside
  `crossbeam-epoch 0.9.18` (transitive via Rayon's work-stealing deque),
  unrelated to this crate.
- `-Zmiri-disable-isolation` - Rayon queries CPU topology at startup.
- `-Zmiri-ignore-leaks` - Rayon's worker pool does not join its threads at
  process exit; the leak warning is benign.

`test_par_iter` performs 24,000 parallel pushes per test under Miri's
single-threaded interpreter, so it takes around 9 minutes. For a fast UB
check during development run only the coverage suite (around 9 seconds):

```sh
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-disable-isolation -Zmiri-ignore-leaks" \
    cargo +nightly miri test --test test_coverage
```

## License

Dual-licensed under [Apache-2.0][apache] or [LGPL-2.1-or-later][lgpl] at
your option.

[frontier]: https://docs.rs/parallel_frontier/latest/parallel_frontier/struct.Frontier.html
[shard]: https://docs.rs/parallel_frontier/latest/parallel_frontier/struct.Shard.html
[producer]: https://docs.rs/parallel_frontier/latest/parallel_frontier/struct.FrontierProducer.html
[iter-fn]: https://docs.rs/parallel_frontier/latest/parallel_frontier/struct.Frontier.html#method.iter
[par-iter-fn]: https://docs.rs/parallel_frontier/latest/parallel_frontier/struct.Frontier.html#method.par_iter
[push]: https://docs.rs/parallel_frontier/latest/parallel_frontier/struct.Frontier.html#method.push
[push-on-thread]: https://docs.rs/parallel_frontier/latest/parallel_frontier/struct.Frontier.html#method.push_on_thread
[as-ref]: https://docs.rs/parallel_frontier/latest/parallel_frontier/struct.Frontier.html#impl-AsRef%3C%5BShard%3CT%3E%5D%3E-for-Frontier%3C'_,+T%3E
[as-mut]: https://docs.rs/parallel_frontier/latest/parallel_frontier/struct.Frontier.html#impl-AsMut%3C%5BShard%3CT%3E%5D%3E-for-Frontier%3C'_,+T%3E
[std-extend]: https://doc.rust-lang.org/std/iter/trait.Extend.html
[rayon-tindex]: https://docs.rs/rayon/latest/rayon/fn.current_thread_index.html
[sux-atomic]: https://docs.rs/sux/latest/sux/bits/struct.AtomicBitVec.html
[tb]: https://github.com/rust-lang/miri#tree-borrows
[apache]: https://www.apache.org/licenses/LICENSE-2.0
[lgpl]: https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html
