use std::cmp::PartialOrd;
use std::cmp::Ordering::Equal;

use self::BkdTree::{Node, Leaf};
use crate::spatial::Dimensions;

pub trait Element {
    type Item;
    type Ray: Ray;
    fn get_bounds_interval(&self, axis: <Self::Ray as Ray>::Dim) -> (f64, f64);
    fn intersect(&self, ray: &Self::Ray) -> Option<(f64, Self::Item)>;
}

impl<E: Element> Element for Option<E> {
    type Item = E::Item;
    type Ray = E::Ray;

    fn get_bounds_interval(&self, axis: <Self::Ray as Ray>::Dim) -> (f64, f64) {
        self.as_ref().map(|e| e.get_bounds_interval(axis)).unwrap()
    }

    fn intersect(&self, ray: &Self::Ray) -> Option<(f64, Self::Item)> {
        self.as_ref().and_then(|e| e.intersect(ray))
    }
}

pub trait Ray {
    type Dim: Dimensions;
    fn plane_intersections(&self, min: f64, max: f64, axis: Self::Dim) -> Option<(f64, f64)>;
    fn plane_distance(&self, min: f64, max: f64, axis: Self::Dim) -> (f64, f64);
}

pub enum BkdTree<E: Element> {
    Node {
        beginning: f64,
        end: f64,
        axis: <E::Ray as Ray>::Dim,
        left: Box<BkdTree<E>>,
        right: Box<BkdTree<E>>
    },

    Leaf {
        beginning: f64,
        end: f64,
        axis: <E::Ray as Ray>::Dim,
        elements: Vec<E>
    }
}

impl<E: Element> BkdTree<E> {
    pub fn new<I>(elements: I, arrity: usize) -> BkdTree<E> where
        I: IntoIterator<Item = E>
    {
        let mut elements: Vec<_> = elements.into_iter().map(|e| Some(e)).collect();
        construct_tree(&mut elements, <E::Ray as Ray>::Dim::first(), arrity, 0)
    }

    pub fn find(&self, ray: &E::Ray) -> Option<(E::Item, &E)> {
        let epsilon = 0.000001;
        let mut result = None;

        let (near, far) = self.distance(ray);
        if far < epsilon {
            return None;
        }

        let mut t_hit = 1.0/0.0;
        let mut stack = vec![(self, epsilon.max(near), far)];

        loop {
            let (node, near, far) = match stack.pop() {
                Some(node) => node,
                None => break
            };

            
            if near > t_hit || far < epsilon {
                continue;
            }
            
            match node {
                &Node { ref left, ref right, .. } => {
                    let (first, first_near, first_far, second, second_near, second_far) = order(&**left, &**right, ray);

                    if second_near <= t_hit && second_far >= near {
                        stack.push((second, second_near.max(near), second_far));
                    }

                    if first_near <= t_hit && first_far >= near {
                        stack.push((first, first_near.max(near), first_far));
                    }
                },
                &Leaf { ref elements, .. } => {
                    for (element, (new_hit, r)) in elements.iter().filter_map(|e| e.intersect(ray).map(|r| (e, r))) {
                        if new_hit > epsilon && new_hit < t_hit {
                            t_hit = new_hit;
                            result = Some((r, element));
                        }
                    }
                }
            }
        }

        result
    }

    pub fn distance(&self, ray: &E::Ray) -> (f64, f64) {
        match *self {
            Node {beginning, end, axis, ..} => ray.plane_distance(beginning, end, axis),
            Leaf {beginning, end, axis, ..} => ray.plane_distance(beginning, end, axis)
        }
    }
}

fn construct_tree<E: Element>(elements: &mut [Option<E>], axis: <E::Ray as Ray>::Dim, arrity: usize, depth: usize) -> BkdTree<E> {
    if elements.len() <= arrity {
        let elements: Vec<_> = elements.iter_mut().filter_map(|e| e.take()).collect();
        let (beginning, end) = get_total_bounds(&elements, axis);

        Leaf {
            beginning: beginning,
            end: end,
            axis: axis,
            elements: elements
        }
    } else {
        elements.sort_by(|a, b| {
            if let (Some(a), Some(b)) = (a.as_ref(), b.as_ref()) {
                let (a_min, a_max) = a.get_bounds_interval(axis);
                let a_mean = (a_min + a_max) / 2.0;

                let (b_min, b_max) = b.get_bounds_interval(axis);
                let b_mean = (b_min + b_max) / 2.0;

                a_mean.partial_cmp(&b_mean).unwrap_or(Equal)
            } else {
                unreachable!()
            }
        });

        let (beginning, end) = get_total_bounds(elements, axis);

        let len = elements.len();
        let median = len / 2;

        let (left, right) = elements.split_at_mut(median);

        Node {
            beginning: beginning,
            end: end,
            axis: axis,
            left: Box::new(construct_tree(left, axis.next(), arrity, depth + 1)),
            right: Box::new(construct_tree(right, axis.next(), arrity, depth + 1))
        }
    }
}

fn get_total_bounds<E: Element>(elements: &[E], axis: <E::Ray as Ray>::Dim) -> (f64, f64) {
    elements.iter().fold((1.0f64/0.0, -1.0f64/0.0), |(begin, end), element| {
        let (e_begin, e_end) = element.get_bounds_interval(axis);
        (begin.min(e_begin), end.max(e_end))
    })
}

#[inline]
fn order<'a, E: Element>(a: &'a BkdTree<E>, b: &'a BkdTree<E>, ray: &E::Ray) -> (&'a BkdTree<E>, f64, f64, &'a BkdTree<E>, f64, f64) {
    let (a_near, a_far) = a.distance(ray);
    let (b_near, b_far) = b.distance(ray);

    let a_dist = a_near + a_far;
    let b_dist = b_near + b_far;

    if a_dist < b_dist {
        (a, a_near, a_far, b, b_near, b_far)
    } else {
        (b, b_near, b_far, a, a_near, a_far)
    }
}