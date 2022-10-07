use std::convert::TryFrom;

use rayon::{iter::plumbing::UnindexedProducer, prelude::*};

mod iter;
pub use iter::*;

mod par_iter;
pub use par_iter::*;

#[derive(Debug)]
pub struct Frontier<T> {
    data: Vec<Vec<T>>,
}

impl<T> PartialEq for Frontier<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| a.eq(b))
    }
}

impl<T> TryFrom<Vec<Vec<T>>> for Frontier<T> {
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
        Ok(Self { data: value })
    }
}

impl<T> From<Vec<T>> for Frontier<T> {
    /// Create a frontier from the provided vector of elements.
    fn from(value: Vec<T>) -> Self {
        let mut frontier = Frontier::default();
        frontier.data[0] = value;
        frontier
    }
}

impl<T> Into<Vec<Vec<T>>> for Frontier<T> {
    /// Converts and consumes the frontier into a vector of vectors.
    fn into(self) -> Vec<Vec<T>> {
        self.data
    }
}

impl<T> Into<Vec<T>> for Frontier<T>
where
    T: Clone,
{
    /// Converts and consumes the frontier into a vector of vectors.
    fn into(self) -> Vec<T> {
        self.data.concat()
    }
}

impl<T> core::default::Default for Frontier<T> {
    /// Create default frontier object with `system_number_of_threads` empty sub-vectors.
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Frontier<T> {
    /// Create new frontier object with `system_number_of_threads` empty sub-vectors.
    pub fn new() -> Self {
        let n_threads = Frontier::<T>::system_number_of_threads();
        Frontier {
            data: (0..n_threads).map(|_| Vec::new()).collect::<Vec<_>>(),
        }
    }

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
        }
    }

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
        unsafe { (*((&self.data[thread_id]) as *const Vec<T> as *mut Vec<T>)).push(value) };
    }

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
        unsafe { (*((&self.data[thread_id]) as *const Vec<T> as *mut Vec<T>)).pop() }
    }

    /// Returns number of the threads, i.e. subvectors, in frontier objects.
    pub fn number_of_threads(&self) -> usize {
        self.data.len()
    }

    /// Returns system number of the threads, i.e. subvectors, in frontier objects.
    pub fn system_number_of_threads() -> usize {
        rayon::current_num_threads().max(1)
    }

    /// Returns total length of the frontier, i.e. the total number of elements in all sub-vectors.
    pub fn len(&self) -> usize {
        self.data.iter().map(|v| v.len()).sum()
    }

    /// Returns whether frontier is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears all sub-vectors, maintaining the reached vector capacity.
    pub fn clear(&mut self) {
        self.data.iter_mut().for_each(|v| v.clear());
    }

    /// Shrinks to fit all sub-vectors.
    pub fn shrink_to_fit(&mut self) {
        self.data.iter_mut().for_each(|v| v.shrink_to_fit());
    }

    /// Converts the frontier into a sequential iterator of the elements.
    pub fn iter(&self) -> FrontierIter<'_, T> {
        FrontierIter::new(self)
    }

    /// Iter the sub-vectors sequentially.
    pub fn iter_vectors(&self) -> impl Iterator<Item = &Vec<T>> + '_ {
        self.data.iter()
    }

    /// Returns vector with the sizes of each subvector.
    pub fn vector_sizes(&self) -> Vec<usize> {
        self.data.iter().map(|v| v.len()).collect::<Vec<_>>()
    }

    /// Converts the frontier into a parallel iterator of the elements.
    pub fn par_iter(&self) -> FrontierParIter<'_, T> {
        FrontierParIter::new(self)
    }
}

impl<T> Frontier<T>
where
    T: Send + Sync,
{
    /// Iter the sub-vectors in parallel.
    pub fn par_iter_vectors(&self) -> impl IndexedParallelIterator<Item = &Vec<T>> + '_ {
        self.data.par_iter()
    }

    /// Iter the sub-vectors in parallel and mutably.
    pub fn par_iter_vectors_mut(
        &mut self,
    ) -> impl IndexedParallelIterator<Item = &mut Vec<T>> + '_ {
        self.data.par_iter_mut()
    }

    /// Iter and consume the sub-vectors in parallel.
    pub fn into_par_iter_vectors(self) -> impl IndexedParallelIterator<Item = Vec<T>> {
        self.data.into_par_iter()
    }
}
