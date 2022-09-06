# Parallel Frontier
Unordered vector of elements with constant time concurrent push. 

This struct should be used when you intend to create a vector in parallel but you
do not have the constraint of having the elements in this vector sorted by the
order they have been inserted.

This struct avoids to copy resulting sub-vectors into a single large one,
as it is done for instance in the [Rayon](https://docs.rs/rayon/latest/rayon/)'s `collect::<Vec<_>>()`
operation. 