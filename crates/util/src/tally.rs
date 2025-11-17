use std::collections::BTreeMap;

pub struct Tally<K: Copy + Ord>(BTreeMap<K, f64>);

impl<K: Copy + Ord> Default for Tally<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord + Copy> Tally<K> {
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn set(&mut self, key: K, value: f64) {
        if value == 0.0 {
            self.0.remove(&key);
        } else {
            self.0.insert(key, value);
        }
    }

    pub fn get(&self, key: K) -> f64 {
        self.0.get(&key).copied().unwrap_or(0.)
    }

    pub fn modify(&mut self, key: K, f: impl FnOnce(f64) -> f64) {
        let x = self.get(key);
        self.set(key, f(x));
    }

    pub fn iter(&self) -> impl Iterator<Item = (K, f64)> + use<'_, K> {
        self.0.iter().map(|(k, v)| (*k, *v))
    }

    pub fn add_one(&mut self, key: K, value: f64) {
        self.modify(key, |x| x + value);
    }
}
