use crate::prelude::*;
use rayon::iter::plumbing::UnindexedProducer;
use rayon::iter::plumbing::Producer;
use std::sync::Arc;

pub struct FrontierIter<'a, T> {
    father: &'a Frontier<T>,

    vec_idx_start: usize,
    value_idx_start: usize,

    // inclusive
    vec_idx_end: usize,
    // esclusive
    value_idx_end: usize,

    cumulative_lens: Arc<Vec<usize>>,
}

impl<'a, T> core::fmt::Debug for FrontierIter<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frontieriter")
            .field("vec_idx_start", &self.vec_idx_start)
            .field("value_idx_start", &self.value_idx_start)
            .field("vec_idx_end", &self.vec_idx_end)
            .field("value_idx_end", &self.value_idx_end)
            .field("cumulative_lens", &self.cumulative_lens)
            .finish()
    }
}

impl<'a, T> FrontierIter<'a, T> {
    pub fn new(father: &'a Frontier<T>) -> Self {
        FrontierIter {
            father,

            vec_idx_start: 0,
            value_idx_start: 0,

            vec_idx_end: father.number_of_threads() - 1,
            value_idx_end: father.as_ref().last().unwrap().len(),

            cumulative_lens: Arc::new(
                father
                    .as_ref()
                    .iter()
                    .map(|v| v.len())
                    .scan(0, |acc, val| {
                        let res = *acc;
                        *acc += val;
                        Some(res)
                    })
                    .collect::<Vec<_>>(),
            ),
        }
    }

    pub fn len(&self) -> usize {
        let start_idx = self.cumulative_lens[self.vec_idx_start] + self.value_idx_start;
        let end_idx = self.cumulative_lens[self.vec_idx_end] + self.value_idx_end;
        end_idx - start_idx
    }
}

impl<'a, T> core::iter::ExactSizeIterator for FrontierIter<'a, T> {}

impl<'a, T> core::iter::Iterator for FrontierIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let mut current_vec;
        loop {
            // if we finished the values
            if self.vec_idx_start >= self.father.number_of_threads() {
                return None;
            }

            current_vec = &self.father.as_ref()[self.vec_idx_start];

            if self.vec_idx_start == self.vec_idx_end && self.value_idx_start >= self.value_idx_end
            {
                return None;
            }

            if self.value_idx_start < current_vec.len() {
                break;
            }

            self.value_idx_start = 0;
            self.vec_idx_start += 1;
        }

        let result = &current_vec[self.value_idx_start];
        self.value_idx_start += 1;
        Some(result)
    }

    fn count(self) -> usize {
        self.len()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len(), Some(self.len()))
    }
}

impl<'a, T> core::iter::DoubleEndedIterator for FrontierIter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            // if we finished the values
            if self.vec_idx_end == 0 && self.value_idx_end == 0 {
                return None;
            }

            if self.vec_idx_start >= self.vec_idx_end && self.value_idx_start >= self.value_idx_end
            {
                return None;
            }

            if self.value_idx_end > 0 {
                break;
            }

            self.vec_idx_end -= 1;
            self.value_idx_end = self.father.as_ref()[self.vec_idx_end].len();
        }

        let result = &self.father.as_ref()[self.vec_idx_end][self.value_idx_end];
        self.value_idx_end -= 1;
        Some(result)
    }
}

impl<'a, T: Sync> UnindexedProducer for FrontierIter<'a, T> {
    type Item = &'a T;

    /// Split the file in two approximately balanced streams
    fn split(mut self) -> (Self, Option<Self>) {
        // Check if it's reasonable to split
        if self.len() < 2 {
            return (self, None);
        }

        let start_idx = self.cumulative_lens[self.vec_idx_start] + self.value_idx_start;
        let end_idx = self.cumulative_lens[self.vec_idx_end] + self.value_idx_end;

        let split_idx = (start_idx + end_idx) / 2;

        debug_assert!(split_idx < end_idx);
        debug_assert!(start_idx < split_idx);
        debug_assert!(
            split_idx < self.father.len(),
            "start_idx: {} end_idx: {} split_idx: {} father len:{}",
            start_idx,
            end_idx,
            split_idx,
            self.father.len()
        );

        match self.cumulative_lens.binary_search(&split_idx) {
            // the split happens at the margin between two vectors
            Ok(vec_idx_mid) => {
                // high part
                let new_iter = Self {
                    father: self.father,

                    vec_idx_start: vec_idx_mid,
                    value_idx_start: 0,

                    vec_idx_end: self.vec_idx_end,
                    value_idx_end: self.value_idx_end,

                    cumulative_lens: self.cumulative_lens.clone(),
                };

                // low part
                self.vec_idx_end = vec_idx_mid - 1;
                self.value_idx_end = self.father.as_ref()[self.vec_idx_end].len();

                // return the two halfs
                debug_assert_ne!(self.len(), 0);
                debug_assert_ne!(new_iter.len(), 0);
                (self, Some(new_iter))
            }
            // the split point is inside a vector so gg ez
            Err(vec_idx_mid) => {
                let vec_idx_mid = vec_idx_mid - 1;
                let value_idx_mid = split_idx - self.cumulative_lens[vec_idx_mid];

                // high part
                let new_iter = Self {
                    father: self.father,

                    vec_idx_start: vec_idx_mid,
                    value_idx_start: value_idx_mid,

                    vec_idx_end: self.vec_idx_end,
                    value_idx_end: self.value_idx_end,

                    cumulative_lens: self.cumulative_lens.clone(),
                };

                // low part
                self.vec_idx_end = vec_idx_mid;
                self.value_idx_end = value_idx_mid;

                // return the two halfs
                debug_assert_ne!(self.len(), 0);
                debug_assert_ne!(new_iter.len(), 0);
                (self, Some(new_iter))
            }
        }
    }

    fn fold_with<F>(self, folder: F) -> F
    where
        F: rayon::iter::plumbing::Folder<Self::Item>,
    {
        folder.consume_iter(self)
    }
}

impl<'a, T: Sync> Producer for FrontierIter<'a, T> {
    type Item = &'a T;
    type IntoIter = Self;

    fn into_iter(self) -> Self::IntoIter {
        self
    }

    fn split_at(mut self, index: usize) -> (Self, Self) {
        let start_idx = self.cumulative_lens[self.vec_idx_start] + self.value_idx_start;
        let split_idx = index + start_idx;
        match self.cumulative_lens.binary_search(&split_idx) {
            // the split happens at the margin between two vectors
            Ok(vec_idx_mid) => {
                // high part
                let new_iter = Self {
                    father: self.father,

                    vec_idx_start: vec_idx_mid,
                    value_idx_start: 0,

                    vec_idx_end: self.vec_idx_end,
                    value_idx_end: self.value_idx_end,

                    cumulative_lens: self.cumulative_lens.clone(),
                };

                // low part
                self.vec_idx_end = vec_idx_mid - 1;
                self.value_idx_end = self.father.as_ref()[self.vec_idx_end].len();

                // return the two halfs
                debug_assert_ne!(self.len(), 0);
                debug_assert_ne!(new_iter.len(), 0);
                (self, new_iter)
            }
            // the split point is inside a vector so gg ez
            Err(vec_idx_mid) => {
                let vec_idx_mid = vec_idx_mid - 1;
                let value_idx_mid = split_idx - self.cumulative_lens[vec_idx_mid];

                // high part
                let new_iter = Self {
                    father: self.father,

                    vec_idx_start: vec_idx_mid,
                    value_idx_start: value_idx_mid,

                    vec_idx_end: self.vec_idx_end,
                    value_idx_end: self.value_idx_end,

                    cumulative_lens: self.cumulative_lens.clone(),
                };

                // low part
                self.vec_idx_end = vec_idx_mid;
                self.value_idx_end = value_idx_mid;

                // return the two halfs
                debug_assert_ne!(self.len(), 0);
                debug_assert_ne!(new_iter.len(), 0);
                (self, new_iter)
            }
        }
    }
}
