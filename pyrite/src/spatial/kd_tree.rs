use std::cmp::Ordering;
use std::slice;
use spatial::Dimensions;

pub trait Element {
    type Point: Point;

    fn position(&self) -> Self::Point;
    fn sq_distance(&self, point: &Self::Point) -> f64;
}

pub trait Point {
    type Dim: Dimensions;

    fn get(&self, axis: Self::Dim) -> f64;
}

pub enum KdTree<E: Element> {
    Node {
        axis: <E::Point as Point>::Dim,
        plane: f64,
        left: Box<KdTree<E>>,
        right: Box<KdTree<E>>,
    },
    Leaf(Vec<E>)
}

impl<E: Element> KdTree<E> {
    pub fn new<I>(elements: I, arrity: usize) -> KdTree<E> where
        I: IntoIterator<Item=E>
    {
        let mut elements: Vec<_> = elements.into_iter().map(Some).collect();
        construct_tree(&mut elements, <E::Point as Point>::Dim::first(), arrity)
    }

    pub fn neighbors<'a>(&'a self, point: &'a E::Point, radius: f64) -> Neighbors<E> {
        Neighbors {
            stack: vec![self],
            current: None,
            radius: radius,
            point: point,
        }
    }
}

pub struct Neighbors<'a, E: Element + 'a>{
    stack: Vec<&'a KdTree<E>>,
    current: Option<slice::Iter<'a, E>>,
    radius: f64,
    point: &'a E::Point,
}

impl<'a, E: Element> Iterator for Neighbors<'a, E> {
    type Item = &'a E;

    fn next(&mut self) -> Option<&'a E> {
        while !self.stack.is_empty() {
            let mut current = self.current.take().or_else(|| {
                while let Some(node) = self.stack.pop() {
                    match *node {
                        KdTree::Node { axis, plane, ref left, ref right } => {
                            let p = self.point.get(axis);
                            let d = p - plane;
                            if d < 0.0 {
                                if d.abs() <= self.radius {
                                    self.stack.push(right);
                                }
                                self.stack.push(left);
                            } else {
                                if d.abs() <= self.radius {
                                    self.stack.push(left);
                                }
                                self.stack.push(right);
                            }
                        },
                        KdTree::Leaf(ref elements) => return Some(elements.iter()),
                    }
                }

                None
            });

            let next = current.as_mut().and_then(|c| {
                while let Some(e) = c.next() {
                    if e.sq_distance(self.point) <= self.radius * self.radius {
                        return Some(e)
                    }
                }

                None
            });
            if next.is_some() {
                self.current = current;
                return next
            }
        }

        None
    }
}

fn construct_tree<E: Element>(elements: &mut [Option<E>], axis: <E::Point as Point>::Dim, arrity: usize) -> KdTree<E> {
    let mut stack = vec![(axis, elements)];
    let mut parents: Vec<(f64, <E::Point as Point>::Dim, Option<KdTree<E>>, Option<KdTree<E>>)> = vec![];

    while let Some((axis, elements)) = stack.pop() {
        if elements.len() <= arrity {
            let elements: Vec<_> = elements.iter_mut().filter_map(|e| e.take()).collect();
            if let Some((plane, axis, left, right)) = parents.pop() {
                if left.is_none() {
                    parents.push((plane, axis, Some(KdTree::Leaf(elements)), right));
                } else {
                    parents.push((plane, axis, left, Some(KdTree::Leaf(elements))));
                }
            } else {
                return KdTree::Leaf(elements)
            }
        } else {
            elements.sort_by(|a, b| {
                if let (Some(a), Some(b)) = (a.as_ref(), b.as_ref()) {
                    a.position().get(axis).partial_cmp(&b.position().get(axis)).unwrap_or(Ordering::Equal)
                } else {
                    unreachable!()
                }
            });

            let median = elements.len() / 2;
            let plane = elements[median].as_ref().expect("median element doesn't exist").position().get(axis);

            let (left, right) = elements.split_at_mut(median);

            stack.push((axis.next(), right));
            stack.push((axis.next(), left));
            parents.push((plane, axis, None, None));
        }

        while let Some(parent) = parents.pop() {
            if let (plane, axis, Some(left), Some(right)) = parent {
                let node = KdTree::Node {
                    axis: axis,
                    plane: plane,
                    left: Box::new(left),
                    right: Box::new(right)
                };
                if let Some((plane, axis, left, right)) = parents.pop() {
                    if left.is_none() {
                        parents.push((plane, axis, Some(node), right));
                    } else {
                        parents.push((plane, axis, left, Some(node)));
                    }
                } else {
                    return node;
                }
            } else {
                parents.push(parent);
                break;
            }
        }
    }

    KdTree::Leaf(vec![])
}
