use std::cmp::PartialOrd;
use std::cmp::Ordering::Equal;
use std::marker::PhantomData;

use self::BkdTree::{Node, Leaf};

pub trait Element<R: Ray> {
    type Item;
    fn get_bounds_interval(&self, axis: usize) -> (f64, f64);
    fn intersect(&self, ray: &R) -> Option<(f64, Self::Item)>;
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
        element: E,
        _phantom_ray: PhantomData<R>
    }
}

impl<R: Ray, E: Element<R>> BkdTree<R, E> {
    pub fn new(elements: Vec<E>, dimensions: usize) -> BkdTree<R, E> {
        construct_tree(elements, dimensions, 0)
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
                &Leaf { ref element, .. } => {
                    match element.intersect(ray) {
                        Some((new_hit, r)) => if new_hit > epsilon && new_hit < t_hit {
                            t_hit = new_hit;
                            result = Some((r, element));
                        },
                        None => {}
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

fn construct_tree<R: Ray, E: Element<R>>(elements: Vec<E>, dimensions: usize, depth: usize) -> BkdTree<R, E> {
    let mut elements = elements;
    let axis = get_best_axis(&elements, dimensions);

    if elements.len() == 1 {
        let element = elements.pop().unwrap();
        let (beginning, end) = element.get_bounds_interval(axis);

        Leaf {
            beginning: beginning,
            end: end,
            axis: axis,
            element: element,
            _phantom_ray: PhantomData
        }
    } else {
        elements.sort_by(|a, b| {
            let (a_min, a_max) = a.get_bounds_interval(axis);
            let a_mean = (a_min + a_max) / 2.0;

            let (b_min, b_max) = b.get_bounds_interval(axis);
            let b_mean = (b_min + b_max) / 2.0;

            a_mean.partial_cmp(&b_mean).unwrap_or(Equal)
        });

        let (beginning, end) = get_total_bounds(&elements, axis);

        let len = elements.len();
        let median = len / 2;
        let mut element_iter = elements.into_iter();

        let left = element_iter.by_ref().take(median).collect();
        let right = element_iter.take(len - median).collect();

        Node {
            beginning: beginning,
            end: end,
            axis: axis,
            left: Box::new(construct_tree(left, dimensions, depth + 1)),
            right: Box::new(construct_tree(right, dimensions, depth + 1))
        }
    }
}

fn get_total_bounds<R: Ray, E: Element<R>>(elements: &Vec<E>, axis: usize) -> (f64, f64) {
    elements.iter().fold((1.0f64/0.0, -1.0f64/0.0), |(begin, end), element| {
        let (e_begin, e_end) = element.get_bounds_interval(axis);
        (begin.min(e_begin), end.max(e_end))
    })
}

fn get_best_axis<R: Ray, E: Element<R>>(elements: &Vec<E>, dimensions: usize) -> usize {
    let mut scores = Vec::new();

    for axis in 0..dimensions {
        let mut sum = 0.0;

        for i in 0..elements.len() - 1 {
            let (base_min, base_max) = elements[i].get_bounds_interval(axis);

            for j in i + 1..elements.len() {
                let (comp_min, comp_max) = elements[j].get_bounds_interval(axis);
                sum += base_max.min(comp_max) - base_min.max(comp_min);
            }
        }

        scores.push(sum);
    }

    let (index, _) = scores.iter().enumerate().fold((0, 1.0/0.0), |(best, max), (i, &v)| if v < max {(i, v)} else {(best, max)});
    index
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