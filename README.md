# Parallel Frontier

Unordered vector of elements with constant time concurrent push.

## What this is

This struct should be used when you intend to create a vector in parallel but you
do not have the constraint of having the elements in this vector sorted by the
order they have been inserted.

This struct avoids to copy resulting shards into a single large one,
as it is done for instance in the [Rayon](https://docs.rs/rayon/latest/rayon/)'s `collect::<Vec<_>>()`
operation.

Once the frontier has been populated, it is possible to iterate all the elements in it,
both sequentially and in parallel (using Rayon's [`ParallelIterator`](https://docs.rs/rayon/latest/rayon/iter/trait.ParallelIterator.html)). Do note that while the elements are NOT sorted, the sequential iterator
will always maintain a consistent order of the elements,

## What this is NOT

This is NOT a set: the elements in the frontier are NOT necessarily unique and
the lack of order does not imply that this is handled in any way such a set.
