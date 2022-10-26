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

    pub const fn new() -> Self {
        Self {
            free_index: 0,
            items: [Self::INIT; N],
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
