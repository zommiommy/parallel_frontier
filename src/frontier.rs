/*
 * SPDX-FileCopyrightText: 2025 Tommaso Fontana
 * SPDX-FileCopyrightText: 2025 Luca Cappelletti
 *
 * SPDX-License-Identifier: Apache-2.0 OR LGPL-2.1-or-later
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

// `UnsafeCell<Vec<T>>` is not `Sync`. Sharing a `Frontier<T>` between threads
// only sends `&Shard<T>` references that are each used to mutate a *distinct*
// shard, so the only data that ever crosses thread boundaries is `T` itself.
// That is sound exactly when `T: Send`.
unsafe impl<T: Send> Sync for Shard<T> {}

impl<T> Shard<T> {
    #[inline]
    fn new() -> Self {
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
    pub(crate) fn as_ptr(&self) -> *mut Vec<T> {
        self.0.get()
    }

    #[inline]
    pub(crate) fn into_inner(self) -> Vec<T> {
        self.0.into_inner()
    }
}

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

impl<T: Clone> Clone for Shard<T> {
    fn clone(&self) -> Self {
        // Safety: cloning takes `&self`; the API contract requires no
        // concurrent mutation while a `&Frontier` is observed.
        Self::from_vec((**self).clone())
    }
}

/// A queue-like frontier for breath-first visits on graphs that supports
/// constant-time concurrent pushes and parallel iteration.
///
/// A parallel frontier can be built with or without a Rayon [`ThreadPool`]; in
/// the second case, the constructor will use Rayon's global [`ThreadPool`].
///
/// If you need (as it is usually the case) to initialize the parallel frontier,
/// after construction you can use [`AsMut`] to access the shards assigned to
/// each thread and initialize them.
///
/// Threads from the [`ThreadPool`] can [push](Frontier::push) elements to the
/// parallel frontier without synchronization. When all threads have completed
/// their pushes, [`Frontier::iter`] and [`Frontier::par_iter`] can be used to
/// iterate on the content of the parallel frontier.
pub struct Frontier<'a, T> {
    data: Vec<Shard<T>>,
    threads: Option<&'a ThreadPool>,
}

impl<T: core::fmt::Debug> core::fmt::Debug for Frontier<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Frontier")
            .field("data", &self.as_ref())
            .field("threads", &self.threads.is_some())
            .finish()
    }
}

impl<T: Clone> Clone for Frontier<'_, T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            threads: self.threads,
        }
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

impl<T> PartialEq for Frontier<'_, T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| a.eq(b))
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

impl<'a, T> From<Frontier<'a, T>> for Vec<T>
where
    T: Clone,
{
    fn from(val: Frontier<'a, T>) -> Self {
        val.concat()
    }
}

impl<T> core::default::Default for Frontier<'_, T> {
    /// Creates a default parallel frontier with
    /// [`Frontier::system_number_of_threads`] empty shards.
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> Frontier<'_, T> {
    #[inline]
    /// Returns the concatenation of the shards.
    pub fn concat(&self) -> Vec<T> {
        let mut out = Vec::with_capacity(self.len());
        for shard in self.as_ref() {
            out.extend_from_slice(shard);
        }
        out
    }
}

impl<'a, T> Frontier<'a, T> {
    #[inline]
    /// Create a new parallel frontier with
    /// [`Frontier::system_number_of_threads`] empty shards.
    pub fn new() -> Self {
        let n_threads = Frontier::<T>::system_number_of_threads();
        Frontier {
            data: (0..n_threads).map(|_| Shard::new()).collect::<Vec<_>>(),
            threads: None,
        }
    }
    #[inline]
    /// Creates a new parallel frontier with
    /// [`Frontier::system_number_of_threads`] empty shards and given overall
    /// capacity.
    ///
    /// # Implementation details
    ///
    /// The provided capacity is distributed roughly uniformly across the
    /// shards.
    pub fn with_capacity(capacity: usize) -> Self {
        let n_threads = Frontier::<T>::system_number_of_threads();
        Frontier {
            data: (0..n_threads)
                .map(|_| Shard::with_capacity(capacity / n_threads))
                .collect::<Vec<_>>(),
            threads: None,
        }
    }

    #[inline]
    /// Creates a new parallel frontier for the specified [`ThreadPool`] and
    /// given overall capacity.
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

    #[inline]
    /// Pushes an element to the frontier on a given thread id.
    ///
    /// This method is unsafe because you might push values on the queue of
    /// another thread.
    ///
    /// # Implementation details  
    ///
    /// A parallel frontier handles a synchronization free *unordered* vector by
    /// assigning a sub-vector to exactly each thread and letting each thread
    /// handle the push to their subvector. When the `push` method is called
    /// outside of a Rayon thread pool we simply push objects to the first
    /// element in the pool. This is useful when initializing the queue, albeit
    /// we rather suggest to use [`AsMut`] and write directly in the relevant
    /// shards.
    ///
    /// # Arguments
    ///
    /// * `element`: T - Element to be pushed onto of the frontier.
    ///
    /// # Safety
    ///
    /// This method is inherently unsafe because it's the responsibility of the
    /// caller to ensure that the thread id is valid and that the corresponding
    /// thread is not currently using the frontier.
    pub unsafe fn push_on_thread(&self, element: T, thread_id: usize) {
        // Safety: `Shard`'s `UnsafeCell` provides interior mutability, and the
        // caller guarantees no concurrent access to this shard.
        unsafe { (*self.data[thread_id].as_ptr()).push(element) };
    }

    #[inline]
    /// Pushes an element to the parallel frontier.
    ///
    /// # Implementation details
    ///
    /// A parallel frontier handles a synchronization free *unordered* vector by
    /// assigning a shard to exactly each thread and letting each thread handle
    /// the push to their shard. When the `push` method is called outside of a
    /// Rayon thread pool we simply push objects to the first element in the
    /// pool.
    ///
    /// # Arguments
    ///
    /// * `element`: T - Object to be pushed onto of the frontier.
    pub fn push(&self, element: T) {
        unsafe {
            self.push_on_thread(element, self.get_current_thread_index());
        };
    }

    #[inline]
    /// Pops an element from frontier.
    ///
    /// # Implementation details
    ///
    /// A parallel frontier handles a synchronization free *unordered* vector by
    /// assigning a shard to exactly each thread and letting each thread handle
    /// the pop from their shard. When the `pop` method is called outside of a
    /// Rayon thread pool we simply pop objects from the first element in the
    /// pool.
    pub fn pop(&self) -> Option<T> {
        unsafe { self.pop_from_thread(self.get_current_thread_index()) }
    }

    #[inline]
    /// Pop element from frontier.
    ///
    /// # Implementation details  
    ///
    /// A parallel frontier handles a synchronization free *unordered* vector
    /// by assigning a sub-vector to exactly each thread and letting each
    /// thread handle the pop from their subvector.
    /// When the `pop` method is called outside of a Rayon thread pool
    /// we simply pop objects from the first element in the pool.
    ///
    /// # Safety
    ///
    /// This method is inherently unsafe because it's the responsibility of
    /// the caller to ensure that the thread_id is valid and that the
    /// corresponding thread is not currently using the frontier.
    pub unsafe fn pop_from_thread(&self, thread_id: usize) -> Option<T> {
        // Safety: see `push_on_thread`.
        unsafe { (*self.data[thread_id].as_ptr()).pop() }
    }

    #[inline]
    /// Returns the number of the threads, that is, shards, in the parallel
    /// frontier.
    pub fn number_of_threads(&self) -> usize {
        self.data.len()
    }

    #[inline]
    /// Returns the system number of the threads, that is, shards, in
    /// parallel frontiers without a user [`ThreadPool`].
    pub fn system_number_of_threads() -> usize {
        rayon::current_num_threads().max(1)
    }

    #[inline]
    /// Returns the total length of the frontier, that is, the total number of
    /// elements in all shards.
    pub fn len(&self) -> usize {
        self.as_ref().iter().map(|s| s.len()).sum()
    }

    #[inline]
    /// Returns whether frontier is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    /// Clears all shards, maintaining the reached vector capacity.
    pub fn clear(&mut self) {
        self.data.iter_mut().for_each(|s| s.clear());
    }

    #[inline]
    /// Shrinks to fit all shards.
    pub fn shrink_to_fit(&mut self) {
        self.data.iter_mut().for_each(|s| s.shrink_to_fit());
    }

    #[inline]
    /// Returns a sequential iterator on the elements of the parallel frontier.
    pub fn iter(&self) -> FrontierIter<'_, T> {
        FrontierIter::new(self)
    }

    #[inline]
    /// Iterates on the shards sequentially.
    pub fn iter_vectors(&self) -> impl Iterator<Item = &Vec<T>> + '_ {
        self.as_ref().iter().map(|s| &**s)
    }

    #[inline]
    /// Returns the sizes of shards.
    pub fn vector_sizes(&self) -> Vec<usize> {
        self.as_ref().iter().map(|s| s.len()).collect::<Vec<_>>()
    }

    #[inline]
    /// Returns a parallel iterator on the elements of the parallel frontier.
    pub fn par_iter(&self) -> FrontierParIter<'_, T> {
        FrontierParIter::new(self)
    }
}

impl<T> Frontier<'_, T>
where
    T: Send + Sync,
{
    #[inline]
    /// Returns an parallel iterator on references to the shards.
    pub fn par_iter_vectors(&self) -> impl IndexedParallelIterator<Item = &Vec<T>> + '_ {
        self.as_ref().par_iter().map(|s| &**s)
    }

    #[inline]
    /// Returns a parallel iterator on mutable references to the shards.
    pub fn par_iter_vectors_mut(
        &mut self,
    ) -> impl IndexedParallelIterator<Item = &mut Vec<T>> + '_ {
        self.as_mut().par_iter_mut().map(|s| &mut **s)
    }

    #[inline]
    /// Consumes self, returning a parallel iterator on the shards.
    pub fn into_par_iter_vectors(self) -> impl IndexedParallelIterator<Item = Vec<T>> {
        self.data.into_par_iter().map(Shard::into_inner)
    }
}
