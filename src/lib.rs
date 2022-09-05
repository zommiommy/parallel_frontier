use rayon::iter::plumbing::UnindexedProducer;

mod iter;
pub use iter::*;

mod par_iter;
pub use par_iter::*;

pub struct Frontier<T>{
    data: Vec<Vec<T>>,
}

impl<T> core::default::Default for Frontier<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Frontier<T> {
    pub fn new() -> Self{
        let n_threads = rayon::current_num_threads().max(1);
        Frontier {
            data: (0..n_threads).map(|_| Vec::new()).collect::<Vec<_>>(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self{
        let n_threads = rayon::current_num_threads().max(1);
        Frontier {
            data: (0..n_threads).map(|_| Vec::with_capacity(capacity / n_threads)).collect::<Vec<_>>(),
        }
    }

    pub fn push(&self, value: T) {
        let thread_id = rayon::current_thread_index().unwrap_or(0);
        unsafe{
            (*((&self.data[thread_id]) as *const Vec<T> as *mut Vec<T>))
            .push(value)
        };
    }

    pub fn number_of_threads(&self) -> usize {
        self.data.len()
    }

    pub fn len(&self) -> usize {
        self.data.iter().map(|v| v.len()).sum()
    }

    pub fn clear(&mut self) {
        self.data.iter_mut().for_each(|v| v.clear());
    }

    pub fn shrink_to_fit(&mut self) {
        self.data.iter_mut().for_each(|v| v.shrink_to_fit());
    }

    pub fn iter(&self) -> FrontierIter<'_, T>{
        FrontierIter::new(self)
    }

    pub fn par_iter(&self) -> FrontierParIter<'_, T>{
        FrontierParIter::new(self)
    }

    pub fn vector_sizes(&self) -> Vec<usize> {
        self.data.iter().map(|v| v.len()).collect::<Vec<_>>()
    }
}

