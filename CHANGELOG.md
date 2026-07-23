# Change Log

## [0.3.2] - 2026-07-23

### Changed

- License is now `Apache-2.0 OR MIT` (previously
  `Apache-2.0 OR LGPL-2.1-or-later`).

## [0.3.1] - 2026-05-06

### Improved

- `Frontier::extend` no longer round-robins. Every value lands in shard
  0, in iterator order. The previous round-robin layout cost up to one
  heap allocation per shard for tiny inputs (e.g. `extend([root])`
  during a per-component BFS), which dominated `Frontier::new` +
  `extend` overhead — see commit message for the measured impact.
  Round-robin bought nothing observable: every consumer
  (`Frontier::par_iter`, the `Producer` impls) re-splits the conceptual
  flattened sequence regardless of the per-shard layout, so the result
  is invisible to readers. Funneling everything through shard 0 is
  faster, more cache-local, and bounds initialization to at most one
  allocation.

## [0.3.0] - 2026-05-06

### New

- `Shard<T>` is now public and implements `Deref` / `DerefMut` to
  `Vec<T>`, so callers reaching shards through `AsRef` / `AsMut`
  keep ergonomic access to the inner `Vec` API.
- `Frontier<'_, T>` implements `Extend<T>` with round-robin
  distribution across shards. Use `frontier.extend(iter)` to safely
  initialize a frontier from any iterator under `&mut self`,
  replacing the previous `as_mut()[0] = vec` idiom.
- `Frontier::par_clear` and `Frontier::par_shrink_to_fit` parallelize
  the corresponding sequential methods over the shards (gated on
  `T: Send`). Worth using when `T` has a non-trivial `Drop` or each
  shard is large enough that `realloc` dominates.
- `FrontierIter` now implements `FusedIterator`, and the hot iterator
  methods are `#[inline]` so cross-crate users get them folded into
  the caller.

### Changed

- **Breaking**: `Shard<T>` is now `#[repr(align(64))]` instead of
  `#[repr(transparent)]`. Each shard occupies its own cache line,
  eliminating false sharing between the per-thread `Vec` headers
  during concurrent pushes.
- **Breaking**: `Frontier<'_, T>` exposes `AsRef<[Shard<T>]>` /
  `AsMut<[Shard<T>]>` instead of `AsRef<[Vec<T>]>` /
  `AsMut<[Vec<T>]>`. Callers should switch to the shard slice and
  rely on `Deref` / `DerefMut` for `Vec` access, or use the new
  `Extend` impl for initialization.

### Improved

- `Frontier::is_empty` now short-circuits on the first non-empty shard
  instead of summing every shard length.
- `Frontier::extend` rotates over `&mut self.data` directly to avoid
  the per-element bounds check and the `if idx == n { idx = 0 }`
  branch.
- `FrontierProducer` stores cumulative shard offsets as
  `Arc<[usize]>` instead of `Arc<Vec<usize>>`, dropping the unused
  capacity field.

## [0.2.0] - 2026-05-05

### Changed

- Split `FrontierIter` into a sequential iterator and a new
  `FrontierProducer` type that owns the rayon `Producer` /
  `UnindexedProducer` impls. Those impls are no longer on
  `FrontierIter`. **Breaking**: callers building producers directly
  must construct `FrontierProducer` instead.

### Fixed

- Wrap per-thread shards in `UnsafeCell` so concurrent pushes satisfy
  Stacked Borrows / Tree Borrows. The previous `push_on_thread` cast a
  `&Vec<T>` to `*mut Vec<T>` and mutated through it, which was UB
  under both aliasing models; Miri now passes.

- Off-by-one in `FrontierIter::next_back`: it indexed at the exclusive
  end before decrementing, panicking the first time any caller
  exercised reverse iteration.

## [0.1.1] - 2025-05-09

### Changed

Changed rust edition from 2024 -> 2021.

## [0.1.0] - 2025-03-31

### New

- First release.
