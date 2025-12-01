use std::marker::PhantomData;

use arrayvec::ArrayVec;
use strum::{EnumCount, IntoEnumIterator};

pub trait EnumMapKey: IntoEnumIterator + EnumCount + Copy + Into<usize> {}

pub struct EnumMap<K: EnumMapKey, V, const N: usize> {
    key_type: PhantomData<K>,
    data: ArrayVec<V, N>,
}

impl<K: EnumMapKey, V: Default, const N: usize> Default for EnumMap<K, V, N> {
    fn default() -> Self {
        Self {
            key_type: PhantomData,
            data: K::iter().map(|_| V::default()).collect(),
        }
    }
}

impl<K: EnumMapKey, V: Default, const N: usize> EnumMap<K, V, N> {
    pub fn with_iter(iter: impl IntoIterator<Item = (K, V)>) -> Self {
        let mut base = Self::default();
        for (k, v) in iter {
            base.set(k, v);
        }
        base
    }
}

impl<K: EnumMapKey, V, const N: usize> EnumMap<K, V, N> {
    pub fn set(&mut self, key: K, value: V) {
        self.data[key.into()] = value;
    }

    pub fn get(&self, key: K) -> &V {
        &self.data[key.into()]
    }

    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (K, &V)> + ExactSizeIterator + DoubleEndedIterator + use<'_, K, V, N>
    {
        K::iter().zip(self.data.iter())
    }

    pub fn iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (K, &mut V)> + ExactSizeIterator + DoubleEndedIterator + use<'_, K, V, N>
    {
        K::iter().zip(self.data.iter_mut())
    }
}

impl<K: EnumMapKey, V: Copy, const N: usize> EnumMap<K, V, N> {
    pub fn update(&mut self, key: K, f: impl FnOnce(V) -> V) {
        self.set(key, f(*self.get(key)));
    }

    pub fn iter_copied(
        &self,
    ) -> impl Iterator<Item = (K, V)> + ExactSizeIterator + DoubleEndedIterator + use<'_, K, V, N>
    {
        K::iter().zip(self.data.iter().copied())
    }
}
