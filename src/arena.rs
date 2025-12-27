use std::{marker::PhantomData, mem::MaybeUninit, num::NonZeroU32};

/// Small arena allocator without deletion
#[derive(Debug, Clone)]
pub struct Arena<T, Tag = T> {
    data: Vec<T>,
    _key_tag: PhantomData<Tag>,
}

#[derive(Debug, Clone)]
pub struct DynArena<T, Tag = T> {
    data: Vec<DynEntry<T>>,
    garbage: Vec<usize>,
    _key_tag: PhantomData<Tag>,
}

#[derive(Debug, Clone)]
struct DynEntry<T> {
    version: NonZeroU32,
    active: bool,
    data: T,
}

/// Typed arena value key
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Key<T>(pub(crate) usize, pub(crate) PhantomData<T>);

/// Typed arena value key for dynamic arena
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct DynKey<T> {
    pub(crate) index: usize,
    pub(crate) version: NonZeroU32,
    _pd: PhantomData<T>,
}

impl<T> DynEntry<T> {
    fn new(data: T) -> Self {
        Self {
            version: NonZeroU32::new(1).unwrap(),
            active: false,
            data,
        }
    }

    fn update(&mut self, data: T) {
        self.version = self.version.checked_add(1).unwrap();
        self.active = true;
        self.data = data;
    }
}

impl<T> DynKey<T> {
    const fn new(index: usize, version: NonZeroU32) -> Self {
        Self {
            index,
            version,
            _pd: PhantomData,
        }
    }

    pub const unsafe fn new_unsafe(index: usize, version: NonZeroU32) -> Self {
        Self {
            index,
            version,
            _pd: PhantomData,
        }
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Default for DynArena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, Tag> Arena<T, Tag> {
    /// Create a new arena
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            _key_tag: PhantomData,
        }
    }

    /// Allocate new block
    pub fn push(&mut self, v: T) -> Key<Tag> {
        let key = Key(self.data.len() as _, PhantomData);
        self.data.push(v);
        key
    }

    pub fn iter(&'_ self) -> impl Iterator<Item = &T> {
        self.data.iter()
    }

    pub fn iter_pairs(&'_ self) -> impl Iterator<Item = (Key<Tag>, &T)> {
        self.data
            .iter()
            .enumerate()
            .map(|(i, d)| (Key(i as _, PhantomData), d))
    }

    /// Allocate new block without initialization
    ///
    /// # Safety
    ///
    /// The allocated block will be zeroed and using it without initialization
    /// may result in undefined behaviour
    pub unsafe fn empty_alloc(&mut self) -> Key<Tag> {
        self.data
            .push(unsafe { MaybeUninit::zeroed().assume_init() });

        Key(self.data.len() as _, PhantomData)
    }

    pub fn get(&self, key: &Key<Tag>) -> Option<&T> {
        self.data.get(key.0)
    }

    pub fn get_mut(&mut self, key: &Key<Tag>) -> Option<&mut T> {
        self.data.get_mut(key.0)
    }

    pub fn get_unchecked(&self, key: &Key<Tag>) -> &T {
        &self.data[key.0]
    }

    pub fn get_mut_unchecked(&mut self, key: &Key<Tag>) -> &mut T {
        &mut self.data[key.0]
    }

    /// Call shrink after you are done allocating to free unused memory
    pub fn shrink(&mut self) {
        self.data.shrink_to_fit();
    }
}

impl<T, Tag> DynArena<T, Tag> {
    /// Create a new arena
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            garbage: Vec::new(),
            _key_tag: PhantomData,
        }
    }

    fn gen_key(&self, index: usize) -> DynKey<Tag> {
        DynKey::new(
            index,
            self.data
                .get(index)
                .map(|e| e.version)
                .unwrap_or_else(|| NonZeroU32::new(1).unwrap()),
        )
    }

    /// Allocate new block
    pub fn push(&mut self, v: T) -> DynKey<Tag> {
        match self.garbage.pop() {
            Some(index) => {
                let entry = &mut self.data[index];
                entry.update(v);
                entry.active = true;
                self.gen_key(index)
            }
            None => {
                let k = self.gen_key(self.data.len());
                let mut entry = DynEntry::new(v);
                entry.active = true;
                self.data.push(entry);
                k
            }
        }
    }

    pub fn delete(&mut self, key: &DynKey<Tag>) {
        self.garbage.push(key.index);
        if let Some(e) = self.data.get_mut(key.index) {
            e.active = false;
        }
    }

    pub unsafe fn remove(&mut self, key: &DynKey<Tag>) -> Option<T> {
        self.delete(key);
        if key.index >= self.data.len() || self.data[key.index].version != key.version {
            return None;
        }
        Some(std::mem::replace(&mut self.data[key.index].data, unsafe {
            MaybeUninit::zeroed().assume_init()
        }))
    }

    pub fn get(&self, key: &DynKey<Tag>) -> Option<&T> {
        self.data
            .get(key.index)
            .filter(|entry| entry.version == key.version)
            .map(|entry| &entry.data)
    }

    pub fn get_mut(&mut self, key: &DynKey<Tag>) -> Option<&mut T> {
        self.data
            .get_mut(key.index)
            .filter(|entry| entry.version == key.version)
            .map(|entry| &mut entry.data)
    }

    pub fn get_unchecked(&self, key: &DynKey<Tag>) -> &T {
        &self.data[key.index].data
    }

    pub fn get_mut_unchecked(&mut self, key: &DynKey<Tag>) -> &mut T {
        &mut self.data[key.index].data
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter().filter(|e| e.active).map(|e| &e.data)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.data
            .iter_mut()
            .filter(|e| e.active)
            .map(|e| &mut e.data)
    }
}
