use std::convert::TryFrom;

use crate::prelude::*;
use rayon::prelude::*;

#[derive(Debug, Clone)]
pub struct MetaFrontier<T> {
    data: Vec<Frontier<T>>,
}

impl<T> AsRef<[Frontier<T>]> for MetaFrontier<T> {
    fn as_ref(&self) -> &[Frontier<T>] {
        self.data.as_ref()
    }
}

impl<T> AsMut<[Frontier<T>]> for MetaFrontier<T> {
    fn as_mut(&mut self) -> &mut [Frontier<T>] {
        self.data.as_mut()
    }
}

impl<T> PartialEq for MetaFrontier<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| a.eq(b))
    }
}

impl<T> TryFrom<Vec<Frontier<T>>> for MetaFrontier<T> {
    type Error = String;

    /// Try to create a frontier from the provided vector of vectors or elements.
    fn try_from(value: Vec<Frontier<T>>) -> Result<Self, Self::Error> {
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
        Ok(Self { data: value })
    }
}

impl<T> From<Vec<T>> for MetaFrontier<T> {
    /// Create a frontier from the provided vector of elements.
    fn from(value: Vec<T>) -> Self {
        let mut meta_frontier = MetaFrontier::default();
        meta_frontier.data[0] = Frontier::from(value);
        meta_frontier
    }
}

impl<T> Into<Vec<Frontier<T>>> for MetaFrontier<T> {
    /// Converts and consumes the frontier into a vector of vectors.
    fn into(self) -> Vec<Frontier<T>> {
        self.data
    }
}

impl<T> core::default::Default for MetaFrontier<T> {
    /// Create default frontier object with `system_number_of_threads` empty sub-vectors.
    fn default() -> Self {
        Self::new()
    }
}

impl<T> MetaFrontier<T> {
    #[inline]
    /// Create new frontier object with `system_number_of_threads` empty sub-vectors.
    pub fn new() -> Self {
        let n_threads = Frontier::<T>::system_number_of_threads();
        Self {
            data: (0..n_threads)
                .map(|_| Frontier::default())
                .collect::<Vec<_>>(),
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
        Self {
            data: (0..n_threads)
                .map(|_| Frontier::with_capacity(capacity / n_threads))
                .collect::<Vec<_>>(),
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
        let thread_id = rayon::current_thread_index().unwrap_or(0);
        self.data[thread_id].push(value);
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
        let thread_id = rayon::current_thread_index().unwrap_or(0);
        self.data[thread_id].pop()
    }

    #[inline]
    /// Returns number of the threads, i.e. subvectors, in frontier objects.
    pub fn number_of_threads(&self) -> usize {
        self.data.len()
    }

    #[inline]
    /// Returns system number of the threads, i.e. subvectors, in frontier objects.
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
    /// Iterates on the i-th sub-frontier associated to the current thread.
    pub fn iter(&self) -> FrontierIter<'_, T> {
        let thread_id = rayon::current_thread_index().unwrap_or(0);
        FrontierIter::new(&self.data[thread_id])
    }

    #[inline]
    /// Iter the sub-vectors sequentially.
    pub fn iter_vectors(&self) -> impl Iterator<Item = &Vec<T>> + '_ {
        self.data
            .iter()
            .flat_map(|frontier| frontier.iter_vectors())
    }

    #[inline]
    /// Returns vector with the sizes of each subvector.
    pub fn vector_sizes(&self) -> Vec<usize> {
        self.data.iter().map(|v| v.len()).collect::<Vec<_>>()
    }

    #[inline]
    /// Iterates in parallel on the i-th sub-frontier associated to the current thread.
    pub fn par_iter(&self) -> FrontierParIter<'_, T> {
        let thread_id = rayon::current_thread_index().unwrap_or(0);
        FrontierParIter::new(&self.data[thread_id])
    }
}

impl<T> MetaFrontier<T>
where
    T: Send + Sync,
{
    #[inline]
    /// Iter the sub-vectors in parallel.
    pub fn par_iter_vectors(&self) -> impl ParallelIterator<Item = &Vec<T>> + '_ {
        self.data
            .par_iter()
            .flat_map(|frontier| frontier.par_iter_vectors())
    }
}
