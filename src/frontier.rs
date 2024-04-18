use std::convert::TryFrom;

use crate::prelude::*;
use rayon::{prelude::*, ThreadPool};

#[derive(Debug, Clone)]
pub struct Frontier<'a, T> {
    data: Vec<Vec<T>>,
    threads: Option<&'a ThreadPool>,
}

impl<'a, T> AsRef<[Vec<T>]> for Frontier<'a, T> {
    fn as_ref(&self) -> &[Vec<T>] {
        self.data.as_ref()
    }
}

impl<'a, T> AsMut<[Vec<T>]> for Frontier<'a, T> {
    fn as_mut(&mut self) -> &mut [Vec<T>] {
        self.data.as_mut()
    }
}

impl<'a, T> PartialEq for Frontier<'a, T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| a.eq(b))
    }
}

impl<'a, T> TryFrom<Vec<Vec<T>>> for Frontier<'a, T> {
    type Error = String;

    /// Try to create a frontier from the provided vector of vectors or elements.
    fn try_from(value: Vec<Vec<T>>) -> Result<Self, Self::Error> {
        if value.len() != Frontier::<T>::system_number_of_threads() {
            return Err(format!(
                concat!(
                    "You have provided a vector with {} sub-vectors ",
                    "to be converted into a Frontier object, but to do ",
                    "so we expected exactly {} sub-vectors."
                ),
                value.len(),
                Frontier::<T>::system_number_of_threads()
            ));
        }
        Ok(Self {
            data: value,
            threads: None,
        })
    }
}

impl<'a, T> From<Vec<T>> for Frontier<'a, T> {
    /// Create a frontier from the provided vector of elements.
    fn from(value: Vec<T>) -> Self {
        let mut frontier = Frontier::default();
        frontier.data[0] = value;
        frontier
    }
}

impl<'a, T> From<Frontier<'a, T>> for Vec<Vec<T>> {
    fn from(val: Frontier<'a, T>) -> Self {
        val.data
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

impl<'a, T> core::default::Default for Frontier<'a, T> {
    /// Create default frontier object with `system_number_of_threads` empty sub-vectors.
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T: Clone> Frontier<'a, T> {
    #[inline]
    /// Converts the frontier into a vector composed of the inner vectors.
    pub fn concat(&self) -> Vec<T> {
        self.data.concat()
    }
}

impl<'a, T> Frontier<'a, T> {
    #[inline]
    /// Create new frontier object with `system_number_of_threads` empty sub-vectors.
    pub fn new() -> Self {
        let n_threads = Frontier::<T>::system_number_of_threads();
        Frontier {
            data: (0..n_threads).map(|_| Vec::new()).collect::<Vec<_>>(),
            threads: None,
        }
    }
    #[inline]
    /// Create new frontier object with `system_number_of_threads` empty sub-vectors.
    ///
    /// # Implementation details
    /// Do note that the provided capacity is distributed roughly uniformely
    /// across the `system_number_of_threads` subvectors.
    pub fn with_capacity(capacity: usize) -> Self {
        let n_threads = Frontier::<T>::system_number_of_threads();
        Frontier {
            data: (0..n_threads)
                .map(|_| Vec::with_capacity(capacity / n_threads))
                .collect::<Vec<_>>(),
            threads: None,
        }
    }

    #[inline]
    /// Create new frontier object for the specified [`ThreadPool`].
    pub fn with_threads(thread_pool: &'a ThreadPool, capacity: Option<usize>) -> Self {
        let n_threads = thread_pool.current_num_threads();
        Frontier {
            data: (0..n_threads)
                .map(|_| Vec::with_capacity(capacity.unwrap_or(0) / n_threads))
                .collect::<Vec<_>>(),
            threads: Some(thread_pool),
        }
    }

    #[inline(always)]
    fn get_current_thread_index(&self) -> usize {
        if let Some(thread_pool) = self.threads {
            // We are using a custom ThreadPool so we want the call to come
            // from the same ThreadPool or from the main thread.
            if let Some(index) = thread_pool.current_thread_index() {
                // The call is from the custom ThreadPool
                index
            } else {
                // The call is from outside the custom ThreadPool so we want
                // it to originate from no pool at all
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
    /// Push value onto frontier.
    ///
    /// # Implementation details  
    /// A frontier object handles a synchronization free *unordered* vector
    /// by assigning a sub-vector to exactly each thread and letting each
    /// thread handle the push to their subvector.
    /// When the `push` method is called outside of a Rayon thread pool
    /// we simply push objects to the first element in the pool.
    ///
    /// # Arguments
    /// * `value`: T - Object to be pushed onto of the frontier.
    pub fn push(&self, value: T) {
        let thread_id = self.get_current_thread_index();
        unsafe { (*((&self.data[thread_id]) as *const Vec<T> as *mut Vec<T>)).push(value) };
    }

    #[inline]
    /// Pop element from frontier.
    ///
    /// # Implementation details  
    /// A frontier object handles a synchronization free *unordered* vector
    /// by assigning a sub-vector to exactly each thread and letting each
    /// thread handle the pop from their subvector.
    /// When the `pop` method is called outside of a Rayon thread pool
    /// we simply pop objects from the first element in the pool.
    pub fn pop(&self) -> Option<T> {
        let thread_id = self.get_current_thread_index();
        unsafe { (*((&self.data[thread_id]) as *const Vec<T> as *mut Vec<T>)).pop() }
    }

    #[inline]
    /// Returns number of the threads, i.e. subvectors, in frontier objects.
    pub fn number_of_threads(&self) -> usize {
        self.data.len()
    }

    #[inline]
    /// Returns system number of the threads, i.e. subvectors, in frontier objects without a user [`ThreadPool`].
    pub fn system_number_of_threads() -> usize {
        rayon::current_num_threads().max(1)
    }

    #[inline]
    /// Returns total length of the frontier, i.e. the total number of elements in all sub-vectors.
    pub fn len(&self) -> usize {
        self.data.iter().map(|v| v.len()).sum()
    }

    #[inline]
    /// Returns whether frontier is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    /// Clears all sub-vectors, maintaining the reached vector capacity.
    pub fn clear(&mut self) {
        self.data.iter_mut().for_each(|v| v.clear());
    }

    #[inline]
    /// Shrinks to fit all sub-vectors.
    pub fn shrink_to_fit(&mut self) {
        self.data.iter_mut().for_each(|v| v.shrink_to_fit());
    }

    #[inline]
    /// Converts the frontier into a sequential iterator of the elements.
    pub fn iter(&self) -> FrontierIter<'_, T> {
        FrontierIter::new(self)
    }

    #[inline]
    /// Iter the sub-vectors sequentially.
    pub fn iter_vectors(&self) -> impl Iterator<Item = &Vec<T>> + '_ {
        self.data.iter()
    }

    #[inline]
    /// Returns vector with the sizes of each subvector.
    pub fn vector_sizes(&self) -> Vec<usize> {
        self.data.iter().map(|v| v.len()).collect::<Vec<_>>()
    }

    #[inline]
    /// Converts the frontier into a parallel iterator of the elements.
    pub fn par_iter(&self) -> FrontierParIter<'_, T> {
        FrontierParIter::new(self)
    }
}

impl<'a, T> Frontier<'a, T>
where
    T: Send + Sync,
{
    #[inline]
    /// Iter the sub-vectors in parallel.
    pub fn par_iter_vectors(&self) -> impl IndexedParallelIterator<Item = &Vec<T>> + '_ {
        self.data.par_iter()
    }

    #[inline]
    /// Iter the sub-vectors in parallel and mutably.
    pub fn par_iter_vectors_mut(
        &mut self,
    ) -> impl IndexedParallelIterator<Item = &mut Vec<T>> + '_ {
        self.data.par_iter_mut()
    }

    #[inline]
    /// Iter and consume the sub-vectors in parallel.
    pub fn into_par_iter_vectors(self) -> impl IndexedParallelIterator<Item = Vec<T>> {
        self.data.into_par_iter()
    }
}
