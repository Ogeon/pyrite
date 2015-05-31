use std::cmp::PartialOrd;
use std::cmp::Ordering::Equal;
use std::marker::PhantomData;

use self::BkdTree::{Node, Leaf};

pub trait Element<R: Ray> {
    type Item;
    fn get_bounds_interval(&self, axis: usize) -> (f64, f64);
    fn intersect(&self, ray: &R) -> Option<(f64, Self::Item)>;
}

impl<R: Ray, E: Element<R>> Element<R> for Option<E> {
    type Item = E::Item;
    fn get_bounds_interval(&self, axis: usize) -> (f64, f64) {
        self.as_ref().map(|e| e.get_bounds_interval(axis)).unwrap()
    }

    fn intersect(&self, ray: &R) -> Option<(f64, Self::Item)> {
        self.as_ref().and_then(|e| e.intersect(ray))
    }
}

pub trait Ray {
    fn plane_intersections(&self, min: f64, max: f64, axis: usize) -> Option<(f64, f64)>;
    fn plane_distance(&self, min: f64, max: f64, axis: usize) -> (f64, f64);
}

pub enum BkdTree<R: Ray, E: Element<R>> {
    Node {
        beginning: f64,
        end: f64,
        axis: usize,
        left: Box<BkdTree<R, E>>,
        right: Box<BkdTree<R, E>>
    },

    Leaf {
        beginning: f64,
        end: f64,
        axis: usize,
        elements: Vec<E>,
        _phantom_ray: PhantomData<R>
    }
}

impl<R: Ray, E: Element<R>> BkdTree<R, E> {
    pub fn new(elements: Vec<E>, dimensions: usize, arrity: usize) -> BkdTree<R, E> {
        let mut elements: Vec<_> = elements.into_iter().map(|e| Some(e)).collect();
        construct_tree(&mut elements, dimensions, arrity, 0)
    }

    pub fn find(&self, ray: &R) -> Option<(E::Item, &E)> {
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

    pub fn distance(&self, ray: &R) -> (f64, f64) {
        match *self {
            Node {beginning, end, axis, ..} => ray.plane_distance(beginning, end, axis),
            Leaf {beginning, end, axis, ..} => ray.plane_distance(beginning, end, axis)
        }
    }
}

fn construct_tree<R: Ray, E: Element<R>>(elements: &mut [Option<E>], dimensions: usize, arrity: usize, depth: usize) -> BkdTree<R, E> {
    let axis = depth % dimensions;

    if elements.len() <= arrity {
        let elements: Vec<_> = elements.iter_mut().filter_map(|mut e| e.take()).collect();
        let (beginning, end) = get_total_bounds(&elements, axis);

        Leaf {
            beginning: beginning,
            end: end,
            axis: axis,
            elements: elements,
            _phantom_ray: PhantomData
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
            left: Box::new(construct_tree(left, dimensions, arrity, depth + 1)),
            right: Box::new(construct_tree(right, dimensions, arrity, depth + 1))
        }
    }
}

fn get_total_bounds<R: Ray, E: Element<R>>(elements: &[E], axis: usize) -> (f64, f64) {
    elements.iter().fold((1.0f64/0.0, -1.0f64/0.0), |(begin, end), element| {
        let (e_begin, e_end) = element.get_bounds_interval(axis);
        (begin.min(e_begin), end.max(e_end))
    })
}

#[inline]
fn order<'a, R: Ray, E: Element<R>>(a: &'a BkdTree<R, E>, b: &'a BkdTree<R, E>, ray: &R) -> (&'a BkdTree<R, E>, f64, f64, &'a BkdTree<R, E>, f64, f64) {
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