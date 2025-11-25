use slotmap::{Key, SecondaryMap};

pub struct OneToOneMap<A, B>
where
    A: Key,
    B: Key,
{
    a_to_b: SecondaryMap<A, B>,
    b_to_a: SecondaryMap<B, A>,
}

impl<A: Key, B: Key> Default for OneToOneMap<A, B> {
    fn default() -> Self {
        Self {
            a_to_b: Default::default(),
            b_to_a: Default::default(),
        }
    }
}

impl<A: Key, B: Key> OneToOneMap<A, B> {
    pub fn insert(&mut self, a: A, b: B) {
        let prev = self.a_to_b.insert(a, b);
        if let Some(prev) = prev {
            let removed = self.b_to_a.remove(prev);
            assert!(removed == Some(a));
        }
        self.b_to_a.insert(b, a);
    }

    pub fn remove_left(&mut self, a: A) {
        if let Some(b) = self.a_to_b.remove(a) {
            let a_prime = self.b_to_a.remove(b);
            assert!(a_prime == Some(a));
        }
    }

    pub fn remove_right(&mut self, b: B) {
        if let Some(a) = self.b_to_a.remove(b) {
            let b_prime = self.a_to_b.remove(a);
            assert!(b_prime == Some(b));
        }
    }

    pub fn get_left(&self, a: A) -> Option<B> {
        self.a_to_b.get(a).copied()
    }

    pub fn get_right(&self, b: B) -> Option<A> {
        self.b_to_a.get(b).copied()
    }
}
