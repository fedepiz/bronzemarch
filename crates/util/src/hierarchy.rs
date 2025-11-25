use std::collections::{BTreeMap, BTreeSet};

use crate::arena::{Arena, ArenaSafe};

pub struct Hierarchy<P, C> {
    parent_to_child: BTreeSet<(P, Entry<C>)>,
    child_to_parent: BTreeMap<C, P>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Entry<T> {
    Min,
    Mid(T),
    Max,
}

impl<T: Copy + Ord> Entry<T> {
    fn new(x: T) -> Self {
        Self::Mid(x)
    }

    fn get(self) -> T {
        match self {
            Self::Mid(x) => x,
            _ => panic!(),
        }
    }
}

impl<P, C> Default for Hierarchy<P, C> {
    fn default() -> Self {
        Self {
            parent_to_child: Default::default(),
            child_to_parent: Default::default(),
        }
    }
}

impl<P, C> Hierarchy<P, C>
where
    P: Copy + Ord + ArenaSafe,
    C: Copy + Ord + ArenaSafe,
{
    pub fn insert(&mut self, parent: P, child: C) {
        if let Some(old_parent) = self.child_to_parent.insert(child, parent) {
            self.parent_to_child
                .remove(&(old_parent, Entry::new(child)));
        }
        self.parent_to_child.insert((parent, Entry::new(child)));
    }

    pub fn children(&self, parent: P) -> impl Iterator<Item = C> + DoubleEndedIterator {
        self.parent_to_child
            .range((parent, Entry::Min)..=(parent, Entry::Max))
            .map(|(_, c)| c.get())
    }

    pub fn parent(&self, child: C) -> Option<P> {
        self.child_to_parent.get(&child).copied()
    }

    pub fn remove_child(&mut self, child: C) {
        if let Some(parent) = self.child_to_parent.remove(&child) {
            self.parent_to_child.remove(&(parent, Entry::new(child)));
        }
    }

    pub fn remove_parents(&mut self, arena: &Arena, parents: &[P]) {
        let children = arena.alloc_iter(parents.iter().flat_map(|&parent| self.children(parent)));
        self.parent_to_child.retain(|(p, _)| !parents.contains(p));
        for child in children {
            self.child_to_parent.remove(&child);
        }
    }
}

impl<T> Hierarchy<T, T>
where
    T: Copy + Ord + ArenaSafe,
{
    pub fn root(&self, mut item: T) -> T {
        loop {
            match self.parent(item) {
                Some(parent) => item = parent,
                None => return item,
            }
        }
    }

    pub fn root_parent(&self, item: T) -> Option<T> {
        self.parent(item).map(|parent| self.root(parent))
    }
}
