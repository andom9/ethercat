pub(crate) const fn const_max(a: usize, b: usize) -> usize {
    if a > b {
        a
    } else {
        b
    }
}

#[derive(Debug)]
pub enum SetOption<H, S>
where
    H: Clone + Into<usize> + Default,
{
    NextFreeIndex(H),
    Exist(S),
}

impl<H, S> Default for SetOption<H, S>
where
    H: Clone + Into<usize> + Default,
{
    fn default() -> Self {
        Self::NextFreeIndex(Default::default())
    }
}

#[derive(Debug)]
pub(crate) struct ArraySet<H, S, const N: usize>
where
    H: Clone + Into<usize> + Default,
{
    free_index: H,
    items: [SetOption<H, S>; N],
}

impl<H, S, const N: usize> ArraySet<H, S, N>
where
    H: Clone + Into<usize> + Default,
    [SetOption<H, S>; N]: Default,
{
    pub fn new() -> Self {
        Self {
            free_index: Default::default(),
            items: Default::default(),
        }
    }

    pub fn add_item(&mut self, item: S) -> Result<H, S> {
        let handle = self.free_index.clone();
        if let Some(option) = self.items.get_mut(handle.clone().into()) {
            if let SetOption::NextFreeIndex(next) = option {
                self.free_index = next.clone();
                *option = SetOption::Exist(item);
                Ok(handle)
            } else {
                unreachable!()
            }
        } else {
            Err(item)
        }
    }

    pub fn remove_item(&mut self, handle: H) -> Option<S> {
        if let Some(option) = self.items.get_mut(handle.clone().into()) {
            match option {
                SetOption::Exist(_) => {
                    let mut next = SetOption::NextFreeIndex(self.free_index.clone());
                    self.free_index = handle;
                    core::mem::swap(option, &mut next);
                    if let SetOption::Exist(item) = next {
                        Some(item)
                    } else {
                        unreachable!()
                    }
                }
                SetOption::NextFreeIndex(_) => None,
            }
        } else {
            None
        }
    }

    pub fn get_item(&self, handle: &H) -> Option<&S> {
        match self.items.get::<usize>(handle.clone().into()) {
            Some(SetOption::Exist(ref item)) => Some(item),
            _ => None,
        }
    }

    pub fn get_item_mut(&mut self, handle: &H) -> Option<&mut S> {
        match self.items.get_mut::<usize>(handle.clone().into()) {
            Some(SetOption::Exist(ref mut item)) => Some(item),
            _ => None,
        }
    }

    pub fn items(&self) -> impl Iterator<Item = &S> {
        self.items.iter().filter_map(|option| {
            if let SetOption::Exist(item) = option {
                Some(item)
            } else {
                None
            }
        })
    }

    pub fn items_mut(&mut self) -> impl Iterator<Item = &mut S> {
        self.items.iter_mut().filter_map(|option| {
            if let SetOption::Exist(item) = option {
                Some(item)
            } else {
                None
            }
        })
    }
}
