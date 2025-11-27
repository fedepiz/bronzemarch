use std::marker::PhantomData;

use arrayvec::ArrayVec;
use strum::EnumCount;

pub trait EnumMapKey: EnumCount + Copy + TryFrom<usize> + Into<usize> + std::fmt::Debug {}

pub struct EnumMap<K: EnumMapKey, V, const N: usize> {
    key_type: PhantomData<K>,
    data: ArrayVec<V, N>,
}

impl<K: EnumMapKey, V: Default, const N: usize> Default for EnumMap<K, V, N> {
    fn default() -> Self {
        Self {
            key_type: PhantomData,
            data: Default::default(),
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
        self.data.iter().enumerate().map(|(idx, v)| {
            let key = match K::try_from(idx) {
                Ok(x) => x,
                _ => panic!(),
            };
            (key, v)
        })
    }

    pub fn iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (K, &mut V)> + ExactSizeIterator + DoubleEndedIterator + use<'_, K, V, N>
    {
        self.data.iter_mut().enumerate().map(|(idx, v)| {
            let key = match K::try_from(idx) {
                Ok(x) => x,
                _ => panic!(),
            };
            (key, v)
        })
    }
}
