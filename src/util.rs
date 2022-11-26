use core::marker::PhantomData;

pub(crate) const fn const_max(a: usize, b: usize) -> usize {
    if a > b {
        a
    } else {
        b
    }
}

#[derive(Debug)]
pub(crate) enum IndexOption<T> {
    NextFreeIndex(usize),
    Exist(T),
}

impl<T> Default for IndexOption<T> {
    fn default() -> Self {
        Self::NextFreeIndex(Default::default())
    }
}

#[derive(Debug)]
pub(crate) struct IndexSet<H, T, const N: usize>
where
    H: Into<usize> + From<usize> + Clone,
{
    free_index: usize,
    items: [IndexOption<T>; N],
    _phantom: PhantomData<H>,
}

impl<H, T, const N: usize> IndexSet<H, T, N>
where
    H: Into<usize> + From<usize> + Clone,
{
    const INIT: IndexOption<T> = IndexOption::NextFreeIndex(0);

    pub fn new() -> Self {
        let mut items = [Self::INIT; N];
        items
            .iter_mut()
            .enumerate()
            .for_each(|(index, v)| *v = IndexOption::NextFreeIndex(index + 1));
        Self {
            free_index: 0,
            items,
            _phantom: PhantomData,
        }
    }

    pub fn add_item(&mut self, item: T) -> Result<H, T> {
        let index = self.free_index.clone();
        if let Some(option) = self.items.get_mut(index) {
            if let IndexOption::NextFreeIndex(next) = option {
                self.free_index = next.clone();
                *option = IndexOption::Exist(item);
                Ok(index.into())
            } else {
                unreachable!()
            }
        } else {
            Err(item)
        }
    }

    pub fn remove_item(&mut self, handle: H) -> Option<T> {
        let index = handle.into();
        if let Some(option) = self.items.get_mut(index) {
            match option {
                IndexOption::Exist(_) => {
                    let mut next = IndexOption::NextFreeIndex(self.free_index.clone());
                    self.free_index = index;
                    core::mem::swap(option, &mut next);
                    if let IndexOption::Exist(item) = next {
                        Some(item)
                    } else {
                        unreachable!()
                    }
                }
                IndexOption::NextFreeIndex(_) => None,
            }
        } else {
            None
        }
    }

    pub fn get_item(&self, handle: &H) -> Option<&T> {
        match self.items.get::<usize>(handle.clone().into()) {
            Some(IndexOption::Exist(ref item)) => Some(item),
            _ => None,
        }
    }

    pub fn get_item_mut(&mut self, handle: &H) -> Option<&mut T> {
        match self.items.get_mut::<usize>(handle.clone().into()) {
            Some(IndexOption::Exist(ref mut item)) => Some(item),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn items(&self) -> impl Iterator<Item = &T> {
        self.items.iter().filter_map(|option| {
            if let IndexOption::Exist(item) = option {
                Some(item)
            } else {
                None
            }
        })
    }

    pub fn items_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.items.iter_mut().filter_map(|option| {
            if let IndexOption::Exist(item) = option {
                Some(item)
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::IndexSet;

    #[test]
    fn index_set_test() {
        let mut set: IndexSet<usize, _, 3> = IndexSet::new();
        let h1 = set.add_item(100).unwrap();
        let item = set.get_item(&h1).unwrap();
        assert_eq!(*item, 100);
        let item = set.get_item_mut(&h1).unwrap();
        *item = 101;
        let item = set.get_item(&h1).unwrap();
        assert_eq!(*item, 101);

        let h2 = set.add_item(200).unwrap();
        let item = set.remove_item(h1).unwrap();
        assert_eq!(item, 101);
        let item2 = set.get_item(&h2).unwrap();
        assert_eq!(*item2, 200);
        assert!(set.get_item(&h1).is_none());

        let _h3 = set.add_item(item).unwrap();
        let _h4 = set.add_item(item).unwrap();
        assert!(set.add_item(item).is_err());
    }
}
