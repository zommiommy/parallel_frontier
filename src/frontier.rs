/*
 * SPDX-FileCopyrightText: 2025 Tommaso Fontana
 * SPDX-FileCopyrightText: 2025 Luca Cappelletti
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use crate::*;
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use rayon::{prelude::*, ThreadPool};

/// Per-thread shard with interior mutability so that concurrent pushes from
/// different threads (each writing to its own shard) are sound.
///
/// The 64-byte alignment makes each shard occupy its own cache line, which
/// prevents false sharing between the per-thread `Vec` headers when threads
/// push concurrently.
///
/// # Soundness contract
///
/// `Shard<T>: Sync` when `T: Send` lets threads share `&Shard<T>` references.
/// Reading the inner `Vec` through `&Shard<T>` (e.g. via [`Deref`]) is sound
/// only while no other thread is mutating the shard through its
/// [`UnsafeCell`]. The owning [`Frontier`] enforces this at the API level by
/// requiring observation and mutation to be serialized by the caller.
#[repr(align(64))]
pub struct Shard<T>(UnsafeCell<Vec<T>>);

impl<T: Clone> Clone for Shard<T> {
    fn clone(&self) -> Self {
        // Safety: cloning takes `&self`; the API contract requires no
        // concurrent mutation while a `&Frontier` is observed.
        Self::from_vec((**self).clone())
    }
}

impl<T> Shard<T> {
    #[inline]
    const fn new() -> Self {
        Self(UnsafeCell::new(Vec::new()))
    }

    #[inline]
    fn with_capacity(cap: usize) -> Self {
        Self(UnsafeCell::new(Vec::with_capacity(cap)))
    }

    #[inline]
    fn from_vec(v: Vec<T>) -> Self {
        Self(UnsafeCell::new(v))
    }

    #[inline]
    pub(crate) const fn as_ptr(&self) -> *mut Vec<T> {
        self.0.get()
    }

    #[inline]
    pub(crate) fn into_inner(self) -> Vec<T> {
        self.0.into_inner()
    }
}

// `UnsafeCell<Vec<T>>` is not `Sync`. Sharing a `Frontier<T>` between threads
// only sends `&Shard<T>` references that are each used to mutate a *distinct*
// shard, so the only data that ever crosses thread boundaries is `T` itself.
// That is sound exactly when `T: Send`.
unsafe impl<T: Send> Sync for Shard<T> {}

impl<T> Deref for Shard<T> {
    type Target = Vec<T>;

    /// Borrow the inner `Vec` immutably.
    ///
    /// See the type-level soundness contract: callers must ensure no
    /// concurrent mutation through other `&Shard<T>` borrows for the
    /// duration of the returned reference.
    #[inline]
    fn deref(&self) -> &Vec<T> {
        // Safety: contract documented on `Shard<T>`.
        unsafe { &*self.0.get() }
    }
}

impl<T> DerefMut for Shard<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Vec<T> {
        self.0.get_mut()
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for Shard<T> {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}

/// A queue-like frontier for breath-first visits on graphs that supports
/// constant-time concurrent pushes and parallel iteration.
///
/// A parallel frontier can be built with or without a Rayon [`ThreadPool`]; in
/// the second case, the constructor will use Rayon's global [`ThreadPool`].
///
/// If you need (as it is usually the case) to initialize the parallel frontier,
/// after construction you can use [`Extend`] to distribute values across the
/// shards, or [`AsMut`] to access individual shards directly.
///
/// Threads from the [`ThreadPool`] can [push](Frontier::push) elements to the
/// parallel frontier without synchronization. When all threads have completed
/// their pushes, [`Frontier::iter`] and [`Frontier::par_iter`] can be used to
/// iterate on the content of the parallel frontier.
pub struct Frontier<'a, T> {
    data: Vec<Shard<T>>,
    threads: Option<&'a ThreadPool>,
}

impl<T: Clone> Clone for Frontier<'_, T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            threads: self.threads,
        }
    }
}

impl<T: Clone> Frontier<'_, T> {
    /// Returns the concatenation of the shards.
    #[inline]
    pub fn concat(&self) -> Vec<T> {
        let mut out = Vec::with_capacity(self.len());
        for shard in self.as_ref() {
            out.extend_from_slice(shard);
        }
        out
    }
}

impl<'a, T> Frontier<'a, T> {
    /// Create a new parallel frontier with
    /// [`Frontier::system_number_of_threads`] empty shards.
    #[inline]
    pub fn new() -> Self {
        let n_threads = Frontier::<T>::system_number_of_threads();
        Frontier {
            data: (0..n_threads).map(|_| Shard::new()).collect::<Vec<_>>(),
            threads: None,
        }
    }

    /// Creates a new parallel frontier with
    /// [`Frontier::system_number_of_threads`] empty shards and given overall
    /// capacity.
    ///
    /// # Implementation Notes
    ///
    /// The provided capacity is distributed roughly uniformly across the
    /// shards.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        let n_threads = Frontier::<T>::system_number_of_threads();
        Frontier {
            data: (0..n_threads)
                .map(|_| Shard::with_capacity(capacity / n_threads))
                .collect::<Vec<_>>(),
            threads: None,
        }
    }

    /// Creates a new parallel frontier for the specified [`ThreadPool`] and
    /// given overall capacity.
    #[inline]
    pub fn with_threads(thread_pool: &'a ThreadPool, capacity: Option<usize>) -> Self {
        let n_threads = thread_pool.current_num_threads();
        Frontier {
            data: (0..n_threads)
                .map(|_| Shard::with_capacity(capacity.unwrap_or(0) / n_threads))
                .collect::<Vec<_>>(),
            threads: Some(thread_pool),
        }
    }

    #[inline(always)]
    fn get_current_thread_index(&self) -> usize {
        if let Some(thread_pool) = self.threads {
            // We are using a custom ThreadPool so we want the call to come from
            // the same ThreadPool or from the main thread.
            if let Some(index) = thread_pool.current_thread_index() {
                // The call is from the custom ThreadPool
                index
            } else {
                // The call is from outside the custom ThreadPool so we want it
                // to originate from no pool at all
                if rayon::current_thread_index().is_some() {
                    panic!("Parallel frontier called from external thread pool")
                } else {
                    0
                }
            }
        } else {
            // We are not using a custom ThreadPool so the global one is used
            rayon::current_thread_index().unwrap_or(0)
        }
    }

    /// Pushes an element to the frontier on a given thread id.
    ///
    /// # Implementation Notes
    ///
    /// A parallel frontier handles a synchronization-free *unordered* vector
    /// by assigning a sub-vector to exactly each thread and letting each
    /// thread handle the push to its subvector. When the [`Frontier::push`]
    /// method is called outside of a Rayon thread pool we simply push objects
    /// to the first element in the pool. This is useful when initializing the
    /// queue, although [`Extend`] is the recommended path.
    ///
    /// # Safety
    ///
    /// This method is inherently unsafe because it is the responsibility of
    /// the caller to ensure that the thread id is valid and that the
    /// corresponding thread is not currently using the frontier.
    #[inline]
    pub unsafe fn push_on_thread(&self, element: T, thread_id: usize) {
        // Safety: `Shard`'s `UnsafeCell` provides interior mutability, and the
        // caller guarantees no concurrent access to this shard.
        unsafe { (*self.data[thread_id].as_ptr()).push(element) };
    }

    /// Pushes an element to the parallel frontier.
    ///
    /// # Implementation Notes
    ///
    /// A parallel frontier handles a synchronization-free *unordered* vector
    /// by assigning a shard to exactly each thread and letting each thread
    /// handle the push to its shard.
    ///
    /// Each call performs a thread-local lookup to map the caller to its
    /// shard. When `push` is called from outside a Rayon thread pool the
    /// element is appended to shard 0; this is convenient for one-off
    /// initialization but turns shard 0 into a serial bottleneck if used
    /// for bulk loading from a non-Rayon thread. For batch initialization
    /// prefer [`Extend`], and inside a tight Rayon loop prefer
    /// [`push_on_thread`](Frontier::push_on_thread) with the thread index
    /// captured once outside the loop.
    #[inline]
    pub fn push(&self, element: T) {
        unsafe {
            self.push_on_thread(element, self.get_current_thread_index());
        };
    }

    /// Pops an element from the frontier.
    ///
    /// # Implementation Notes
    ///
    /// A parallel frontier handles a synchronization-free *unordered* vector
    /// by assigning a shard to exactly each thread and letting each thread
    /// handle the pop from its shard. When the `pop` method is called
    /// outside of a Rayon thread pool we simply pop objects from the first
    /// element in the pool.
    #[inline]
    pub fn pop(&self) -> Option<T> {
        unsafe { self.pop_from_thread(self.get_current_thread_index()) }
    }

    /// Pops an element from the frontier on a given thread id.
    ///
    /// # Implementation Notes
    ///
    /// A parallel frontier handles a synchronization-free *unordered* vector
    /// by assigning a sub-vector to exactly each thread and letting each
    /// thread handle the pop from its subvector. When the `pop` method is
    /// called outside of a Rayon thread pool we simply pop objects from the
    /// first element in the pool.
    ///
    /// # Safety
    ///
    /// This method is inherently unsafe because it is the responsibility of
    /// the caller to ensure that the thread id is valid and that the
    /// corresponding thread is not currently using the frontier.
    #[inline]
    pub unsafe fn pop_from_thread(&self, thread_id: usize) -> Option<T> {
        // Safety: see `push_on_thread`.
        unsafe { (*self.data[thread_id].as_ptr()).pop() }
    }

    /// Returns the number of the threads, that is, shards, in the parallel
    /// frontier.
    #[inline]
    pub fn number_of_threads(&self) -> usize {
        self.data.len()
    }

    /// Returns the system number of the threads, that is, shards, in
    /// parallel frontiers without a user [`ThreadPool`].
    #[inline]
    pub fn system_number_of_threads() -> usize {
        rayon::current_num_threads().max(1)
    }

    /// Returns the total length of the frontier, that is, the total number of
    /// elements in all shards.
    #[inline]
    pub fn len(&self) -> usize {
        self.as_ref().iter().map(|s| s.len()).sum()
    }

    /// Returns whether the frontier is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.as_ref().iter().all(|s| s.is_empty())
    }

    /// Clears all shards, retaining each shard's allocated capacity.
    #[inline]
    pub fn clear(&mut self) {
        self.data.iter_mut().for_each(|s| s.clear());
    }

    /// Shrinks every shard to fit its current length.
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.data.iter_mut().for_each(|s| s.shrink_to_fit());
    }

    /// Returns a sequential iterator on the elements of the parallel frontier.
    #[inline]
    pub fn iter(&self) -> FrontierIter<'_, T> {
        FrontierIter::new(self)
    }

    /// Iterates on the shards sequentially.
    #[inline]
    pub fn iter_vectors(&self) -> impl Iterator<Item = &Vec<T>> + '_ {
        self.as_ref().iter().map(|s| &**s)
    }

    /// Returns the sizes of shards.
    #[inline]
    pub fn vector_sizes(&self) -> Vec<usize> {
        self.as_ref().iter().map(|s| s.len()).collect::<Vec<_>>()
    }

    /// Returns a parallel iterator on the elements of the parallel frontier.
    #[inline]
    pub fn par_iter(&self) -> FrontierParIter<'_, T> {
        FrontierParIter::new(self)
    }
}

impl<T: Send + Sync> Frontier<'_, T> {
    /// Returns a parallel iterator on references to the shards.
    #[inline]
    pub fn par_iter_vectors(&self) -> impl IndexedParallelIterator<Item = &Vec<T>> + '_ {
        self.as_ref().par_iter().map(|s| &**s)
    }

    /// Returns a parallel iterator on mutable references to the shards.
    #[inline]
    pub fn par_iter_vectors_mut(
        &mut self,
    ) -> impl IndexedParallelIterator<Item = &mut Vec<T>> + '_ {
        self.as_mut().par_iter_mut().map(|s| &mut **s)
    }

    /// Consumes self, returning a parallel iterator on the shards.
    #[inline]
    pub fn into_par_iter_vectors(self) -> impl IndexedParallelIterator<Item = Vec<T>> {
        self.data.into_par_iter().map(Shard::into_inner)
    }
}

impl<T: Send> Frontier<'_, T> {
    /// Parallel counterpart of [`Frontier::clear`].
    ///
    /// Each shard is cleared on a Rayon worker. Useful when `T` has a
    /// non-trivial `Drop` (boxed allocations, owned strings, nested
    /// containers); for `T: Copy` the sequential [`Frontier::clear`] is
    /// already O(n_shards) and the Rayon scheduler overhead makes this
    /// version slower.
    #[inline]
    pub fn par_clear(&mut self) {
        self.data.par_iter_mut().for_each(|s| s.clear());
    }

    /// Parallel counterpart of [`Frontier::shrink_to_fit`].
    ///
    /// Each shard's `realloc` happens on a Rayon worker. Worth using when
    /// shards are large enough that the per-shard allocator call dominates
    /// the total cost.
    #[inline]
    pub fn par_shrink_to_fit(&mut self) {
        self.data.par_iter_mut().for_each(|s| s.shrink_to_fit());
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for Frontier<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Frontier")
            .field("data", &self.as_ref())
            .field("threads", &self.threads.is_some())
            .finish()
    }
}

impl<T> AsRef<[Shard<T>]> for Frontier<'_, T> {
    #[inline]
    fn as_ref(&self) -> &[Shard<T>] {
        &self.data
    }
}

impl<T> AsMut<[Shard<T>]> for Frontier<'_, T> {
    #[inline]
    fn as_mut(&mut self) -> &mut [Shard<T>] {
        &mut self.data
    }
}

impl<T: PartialEq> PartialEq for Frontier<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| a.eq(b))
    }
}

impl<T> core::default::Default for Frontier<'_, T> {
    /// Creates a default parallel frontier with
    /// [`Frontier::system_number_of_threads`] empty shards.
    fn default() -> Self {
        Self::new()
    }
}

impl<T> From<Vec<T>> for Frontier<'_, T> {
    /// Create a frontier from the provided vector of elements.
    fn from(value: Vec<T>) -> Self {
        let mut frontier = Frontier::default();
        frontier.data[0] = Shard::from_vec(value);
        frontier
    }
}

impl<'a, T> From<Frontier<'a, T>> for Vec<Vec<T>> {
    fn from(val: Frontier<'a, T>) -> Self {
        val.data.into_iter().map(Shard::into_inner).collect()
    }
}

impl<'a, T: Clone> From<Frontier<'a, T>> for Vec<T> {
    fn from(val: Frontier<'a, T>) -> Self {
        val.concat()
    }
}

impl<T> Extend<T> for Frontier<'_, T> {
    /// Append every element of `iter` to the first shard.
    ///
    /// `&mut self` makes this safe in the presence of [`Shard`]'s interior
    /// mutability: while the call is in progress no other thread can hold
    /// a `&Frontier` and therefore cannot reach a `&Shard` either.
    ///
    /// # Implementation Notes
    ///
    /// Earlier versions distributed values round-robin across all shards.
    /// That bought nothing in practice: every consumer of the frontier
    /// (e.g. [`Frontier::par_iter`]) walks the conceptual flattened
    /// sequence and re-splits the work itself, so the upfront layout is
    /// invisible by the time anyone reads the data. Round-robin also
    /// forced one heap allocation per shard for tiny inputs (e.g. a
    /// single root in a BFS), turning every `extend` into an O(n_threads)
    /// allocator hit. Funneling everything through shard 0 keeps the
    /// elements contiguous (better cache locality for the subsequent
    /// par_iter) and limits initialization to at most one allocation.
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        debug_assert!(
            !self.data.is_empty(),
            "frontier always has at least one shard"
        );
        // Safety: we do not hold any reference into `self.data` across the
        // call to `Vec::extend`; the borrow lives only for the duration of
        // the next statement.
        let shard0 = unsafe { &mut *self.data[0].as_ptr() };
        shard0.extend(iter);
    }
}
