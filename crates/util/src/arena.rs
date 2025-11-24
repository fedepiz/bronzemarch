use bumpalo::Bump;

#[derive(Default)]
pub struct Arena(Bump);

pub type AVec<'a, T> = bumpalo::collections::Vec<'a, T>;

impl Arena {
    pub fn alloc<T: ArenaSafe>(&self, value: T) -> &mut T {
        self.0.alloc(value)
    }

    pub fn alloc_iter<T: ArenaSafe>(&self, iter: impl Iterator<Item = T>) -> &mut [T] {
        let mut vec = AVec::new_in(&self.0);
        vec.extend(iter);
        vec.into_bump_slice_mut()
    }
}

pub trait ArenaSafe {}

impl ArenaSafe for bool {}
impl ArenaSafe for char {}

impl ArenaSafe for i8 {}
impl ArenaSafe for i16 {}
impl ArenaSafe for i32 {}
impl ArenaSafe for i64 {}

impl ArenaSafe for f32 {}
impl ArenaSafe for f64 {}

impl ArenaSafe for u8 {}
impl ArenaSafe for u16 {}
impl ArenaSafe for u32 {}
impl ArenaSafe for u64 {}

impl<T1: ArenaSafe, T2: ArenaSafe> ArenaSafe for (T1, T2) {}
impl<T1: ArenaSafe, T2: ArenaSafe, T3: ArenaSafe> ArenaSafe for (T1, T2, T3) {}
