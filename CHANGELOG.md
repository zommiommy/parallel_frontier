# Change Log

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

First release.
