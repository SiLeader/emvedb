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
            && item < *bottom
        {
            self.items.pop();
            self.items.push(item);
        }
    }

    pub fn into_vec(self) -> Vec<T> {
        let mut items: Vec<T> = self.items.into_iter().map(|Reverse(item)| item).collect();
        items.sort_by(|a, b| b.cmp(a));
        items
    }
}

#[cfg(test)]
mod tests {
    use super::BinaryTopKMinHeap;

    #[test]
    fn keeps_largest_items_in_descending_order() {
        let mut heap = BinaryTopKMinHeap::new(2);

        heap.push(1);
        heap.push(10);
        heap.push(3);

        assert_eq!(heap.into_vec(), vec![10, 3]);
    }
}
