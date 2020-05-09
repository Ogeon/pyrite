use std::mem::transmute;
use std::cmp::PartialOrd;
use std::ops::{Range, Sub, Add};

pub fn pairs<T, F>(v: &mut [T], mut f: F) where F: FnMut(&mut T, &mut T) {
    let ptr = v.as_mut_ptr();
    if v.len() >= 2 {
        for pos in 0..v.len() - 2 {
            let (a, b) = unsafe  { (transmute(ptr.offset(pos as isize)), transmute(ptr.offset(pos as isize + 1))) };
            f(a, b);
        }
    }
}

pub struct BatchRange<N> {
    from: N,
    to: N,
    batch: N
}

impl<N> BatchRange<N> {
    pub fn new(range: Range<N>, batch: N) -> BatchRange<N> {
        BatchRange {
            from: range.start,
            to: range.end,
            batch: batch,
        }
    }
}

impl<N> Iterator for BatchRange<N> where
    N: PartialOrd + Sub<Output=N> + Add<Output=N> + Copy
{
    type Item = N;

    fn next(&mut self) -> Option<N> {
        if self.from < self.to {
            let diff = self.to - self.from;
            if diff < self.batch {
                self.from = self.from + diff;
                Some(diff)
            } else {
                self.from = self.from + self.batch;
                Some(self.batch)
            }
        } else {
            None
        }
    }
}
