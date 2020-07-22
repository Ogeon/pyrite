use cgmath::{EuclideanSpace, InnerSpace, Point3};
use collision::{Aabb, Aabb3, Continuous, Ray3, SurfaceArea, Union};

use super::Dim3;

use crate::math::DIST_EPSILON;

pub(crate) struct Bvh<T> {
    nodes: Vec<FlatBvhNode<T>>,
}

impl<T: Bounded> Bvh<T> {
    pub fn new(items: Vec<T>) -> Bvh<T> {
        if items.is_empty() {
            return Bvh { nodes: Vec::new() };
        }

        let mut stack = Vec::new();
        let mut nodes: Vec<BvhNode<T>> = Vec::new();

        let mut hull = items
            .get(0)
            .map_or_else(Hull::empty, |item| Hull::new(item.aabb()));

        let mut item_refs = Vec::with_capacity(items.len());
        for item in items {
            hull = hull.expand(&item.aabb());
            item_refs.push(item);
        }

        stack.push(StackEntry::Split {
            items: item_refs,
            hull,
        });

        while let Some(entry) = stack.pop() {
            match entry {
                StackEntry::Join { bounding_box } => {
                    let first = nodes
                        .pop()
                        .expect("a first node should be on the node stack");
                    let second = nodes
                        .pop()
                        .expect("a second node should be on the node stack");
                    nodes.push(BvhNode {
                        bounding_box,
                        node_type: BvhNodeType::Node {
                            subtree_size: first.subtree_size() + second.subtree_size() + 2,
                            first: Box::new(first),
                            second: Box::new(second),
                        },
                    });
                }
                StackEntry::Split { mut items, hull } => {
                    if items.len() == 1 {
                        nodes.push(BvhNode {
                            bounding_box: hull.aabbs,
                            node_type: BvhNodeType::Leaf {
                                item: items.pop().unwrap(),
                            },
                        });
                        continue;
                    }

                    // Find the best axis for splitting, and check if the items are too close to each other to be
                    // able to find good clusters.
                    let (split_axis_width, split_axis) = hull.largest_axis();
                    if split_axis_width < DIST_EPSILON {
                        // Split the items evenly between the child nodes.
                        let second_items = items.split_off(items.len() / 2);
                        let first_items = items;
                        let mut first_hull = Hull::new(first_items[0].aabb());
                        for item in &first_items {
                            first_hull = first_hull.expand(&item.aabb());
                        }
                        let mut second_hull = Hull::new(second_items[0].aabb());
                        for item in &second_items {
                            second_hull = second_hull.expand(&item.aabb());
                        }

                        stack.push(StackEntry::Join {
                            bounding_box: hull.aabbs,
                        });
                        stack.push(StackEntry::Split {
                            items: second_items,
                            hull: second_hull,
                        });
                        stack.push(StackEntry::Split {
                            items: first_items,
                            hull: first_hull,
                        });
                    } else {
                        // Partition the axis into a number of evenly distributed buckets the items can fall into.
                        const BUCKETS: usize = 6;
                        let mut buckets: [Option<(Vec<_>, Hull)>; BUCKETS] = Default::default();

                        let min_bound = split_axis.point_element(hull.centroids.min);
                        for item in items {
                            let bounding_box = item.aabb();
                            let position = split_axis.point_element(bounding_box.center());
                            let float_index =
                                BUCKETS as f32 * (position - min_bound) / split_axis_width;
                            let index = (float_index as usize).min(BUCKETS - 1);

                            if let Some((bucket, hull)) = &mut buckets[index] {
                                bucket.push(item);
                                *hull = hull.expand(&bounding_box);
                            } else {
                                buckets[index] = Some((vec![item], Hull::new(bounding_box)));
                            }
                        }

                        // Find the most efficient split.
                        let mut min_cost = f32::INFINITY;
                        let mut min_cost_split = 0;
                        let hull_area = hull.aabbs.surface_area();
                        for index in 1..BUCKETS {
                            let (first_buckets, second_buckets) = buckets.split_at(index);
                            let (first_count, first_area) = get_bucket_stats(first_buckets);
                            let (second_count, second_area) = get_bucket_stats(second_buckets);

                            let cost = (first_area * first_count as f32
                                + second_area * second_count as f32)
                                / hull_area;

                            if cost < min_cost {
                                min_cost_split = index;
                                min_cost = cost;
                            }
                        }

                        let (first_buckets, second_buckets) = buckets.split_at_mut(min_cost_split);
                        let (first_items, first_hull) = merge_buckets(first_buckets);
                        let (second_items, second_hull) = merge_buckets(second_buckets);

                        stack.push(StackEntry::Join {
                            bounding_box: hull.aabbs,
                        });
                        stack.push(StackEntry::Split {
                            items: second_items,
                            hull: second_hull.expect("there should be a second items hull"),
                        });
                        stack.push(StackEntry::Split {
                            items: first_items,
                            hull: first_hull.expect("there should be a first items hull"),
                        });
                    }
                }
            }
        }

        Self {
            nodes: nodes.pop().map_or_else(Vec::new, BvhNode::flatten),
        }
    }
}

impl<T> Bvh<T> {
    pub fn ray_intersect(&self, ray: Ray3<f32>) -> Intersections<T> {
        Intersections {
            nodes: self.nodes.iter(),
            ray,
        }
    }
}

fn get_bucket_stats<T>(buckets: &[Option<(Vec<T>, Hull)>]) -> (usize, f32) {
    let (count, aabb) =
        buckets
            .iter()
            .fold((0, None), |acc: (usize, Option<Aabb3<f32>>), bucket| {
                if let Some((bucket, hull)) = bucket {
                    let aabb = acc.1.map_or(hull.aabbs, |aabb| aabb.union(&hull.aabbs));
                    (acc.0 + bucket.len(), Some(aabb))
                } else {
                    acc
                }
            });
    let area = aabb.as_ref().map_or(0.0, Aabb3::surface_area);

    (count, area)
}

fn merge_buckets<T>(buckets: &mut [Option<(Vec<T>, Hull)>]) -> (Vec<T>, Option<Hull>) {
    buckets.iter_mut().fold(
        (Vec::new(), None),
        |mut acc: (Vec<_>, Option<Hull>), bucket| {
            if let Some((bucket, hull)) = bucket.take() {
                let hull = acc
                    .1
                    .map_or_else(|| hull.clone(), |acc_hull| hull.join(&acc_hull));
                acc.0.extend(bucket);
                (acc.0, Some(hull))
            } else {
                acc
            }
        },
    )
}

pub struct Intersections<'a, T> {
    nodes: std::slice::Iter<'a, FlatBvhNode<T>>,
    ray: Ray3<f32>,
}

impl<'a, T> Intersections<'a, T> {
    pub fn next(&mut self, max_distance: f32) -> Option<&T> {
        let max_sq_distance = max_distance * max_distance;

        // Contains the next node after skipping a few
        let mut next_node = None;

        while let Some(node) = next_node.take().or_else(|| self.nodes.next()) {
            if let Some(intersection) = node.bounding_box.intersection(&self.ray) {
                if (intersection - self.ray.origin).magnitude2() >= max_sq_distance {
                    if node.subtree_size() > 0 {
                        next_node = self.nodes.nth(node.subtree_size());
                    }
                    continue;
                }

                if let FlatBvhNodeType::Leaf { item } = &node.node_type {
                    return Some(item);
                }
            } else if node.subtree_size() > 0 {
                next_node = self.nodes.nth(node.subtree_size());
            }
        }

        None
    }
}

enum StackEntry<T> {
    Split { items: Vec<T>, hull: Hull },
    Join { bounding_box: Aabb3<f32> },
}

struct BvhNode<T> {
    bounding_box: Aabb3<f32>,
    node_type: BvhNodeType<T>,
}

impl<T> BvhNode<T> {
    fn subtree_size(&self) -> usize {
        match self.node_type {
            BvhNodeType::Node { subtree_size, .. } => subtree_size,
            BvhNodeType::Leaf { .. } => 0,
        }
    }

    fn flatten(self) -> Vec<FlatBvhNode<T>> {
        let mut flat_nodes = Vec::new();
        let mut stack = vec![self];

        while let Some(node) = stack.pop() {
            let node_type = match node.node_type {
                BvhNodeType::Node {
                    first,
                    second,
                    subtree_size,
                } => {
                    stack.push(*second);
                    stack.push(*first);
                    FlatBvhNodeType::Node { subtree_size }
                }
                BvhNodeType::Leaf { item } => FlatBvhNodeType::Leaf { item },
            };

            flat_nodes.push(FlatBvhNode {
                bounding_box: node.bounding_box,
                node_type,
            });
        }

        flat_nodes
    }
}

enum BvhNodeType<T> {
    Node {
        first: Box<BvhNode<T>>,
        second: Box<BvhNode<T>>,
        subtree_size: usize,
    },
    Leaf {
        item: T,
    },
}

struct FlatBvhNode<T> {
    bounding_box: Aabb3<f32>,
    node_type: FlatBvhNodeType<T>,
}

impl<T> FlatBvhNode<T> {
    fn subtree_size(&self) -> usize {
        match self.node_type {
            FlatBvhNodeType::Node { subtree_size } => subtree_size,
            FlatBvhNodeType::Leaf { .. } => 0,
        }
    }
}

enum FlatBvhNodeType<T> {
    Node { subtree_size: usize },
    Leaf { item: T },
}

pub(crate) trait Bounded {
    fn aabb(&self) -> Aabb3<f32>;
}

impl<'a, T: Bounded> Bounded for &'a T {
    fn aabb(&self) -> Aabb3<f32> {
        (*self).aabb()
    }
}

#[derive(Clone, Debug)]
struct Hull {
    aabbs: Aabb3<f32>,
    centroids: Aabb3<f32>,
}

impl Hull {
    fn new(aabb: Aabb3<f32>) -> Self {
        Self {
            aabbs: aabb,
            centroids: Aabb3::new(aabb.center(), aabb.center()),
        }
    }

    fn empty() -> Self {
        Self {
            aabbs: Aabb3::new(Point3::origin(), Point3::origin()),
            centroids: Aabb3::new(Point3::origin(), Point3::origin()),
        }
    }

    #[must_use = "a new hull is created"]
    fn expand(&self, aabb: &Aabb3<f32>) -> Self {
        Self {
            aabbs: self.aabbs.union(aabb),
            centroids: self.centroids.grow(aabb.center()),
        }
    }

    #[must_use = "a new hull is created"]
    fn join(&self, other: &Hull) -> Self {
        Self {
            aabbs: self.aabbs.union(&other.aabbs),
            centroids: self.centroids.union(&other.centroids),
        }
    }

    fn largest_axis(&self) -> (f32, Dim3) {
        let dimensions = self.centroids.dim();

        let (max, max_dim) = if dimensions.y > dimensions.x {
            (dimensions.y, Dim3::Y)
        } else {
            (dimensions.x, Dim3::X)
        };

        if dimensions.z > max {
            (dimensions.z, Dim3::Z)
        } else {
            (max, max_dim)
        }
    }
}
