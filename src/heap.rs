use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub(crate) struct BinaryTopKMinHeap<T> {
    items: BinaryHeap<Reverse<T>>,
    k: usize,
}

impl<T> BinaryTopKMinHeap<T>
where
    T: Ord,
{
    pub fn new(k: usize) -> Self {
        Self {
            items: BinaryHeap::with_capacity(k),
            k,
        }
    }

    pub fn push(&mut self, item: T) {
        let item = Reverse(item);
        if self.items.len() < self.k {
            self.items.push(item);
        } else if let Some(bottom) = self.items.peek()
            && item > *bottom
        {
            self.items.pop();
            self.items.push(item);
        }
    }

    pub fn into_vec(self) -> Vec<T> {
        self.items.into_iter().map(|Reverse(item)| item).collect()
    }
}
