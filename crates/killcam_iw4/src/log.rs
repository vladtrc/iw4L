#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Log<T: Copy, const N: usize> {
    items: [Option<T>; N],
    len: usize,
    overflow: usize,
}

impl<T: Copy, const N: usize> Default for Log<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy, const N: usize> Log<T, N> {
    pub const fn new() -> Self {
        Self {
            items: [None; N],
            len: 0,
            overflow: 0,
        }
    }

    pub fn push(&mut self, item: T) {
        if self.len < N {
            self.items[self.len] = Some(item);
            self.len += 1;
        } else {
            self.overflow += 1;
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn overflow(&self) -> usize {
        self.overflow
    }

    pub fn get(&self, i: usize) -> Option<T> {
        if i < self.len { self.items[i] } else { None }
    }

    pub fn iter(&self) -> impl Iterator<Item = T> + '_ {
        (0..self.len).filter_map(move |i| self.items[i])
    }

    pub fn contains(&self, item: &T) -> bool
    where
        T: PartialEq,
    {
        self.iter().any(|x| x == *item)
    }
}
