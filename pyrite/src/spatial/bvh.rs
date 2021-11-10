use cgmath::Point3;
use collision::{Aabb, Aabb3, Contains, Ray3, SurfaceArea, Union};

use super::Dim3;

use crate::math::{aabb_intersection_distance, DIST_EPSILON};

pub(crate) struct Bvh<T> {
    nodes: Vec<FlatBvhNode<T>>,
}

impl<T: Bounded> Bvh<T> {
    pub fn new(mut all_items: Vec<T>) -> Bvh<T> {
        let mut hull = if let Some(item) = all_items.get(0) {
            Hull::new(item.aabb())
        } else {
            return Bvh { nodes: Vec::new() };
        };

        let mut stack = Vec::new();
        let mut nodes: Vec<BvhNode> = Vec::with_capacity(all_items.len());
        let mut bucket_indices = Vec::with_capacity(all_items.len());

        for item in &all_items {
            hull = hull.expand(&item.aabb());
        }

        stack.push(StackEntry::Split {
            begin: 0,
            end: all_items.len(),
            hull,
        });

        while let Some(entry) = stack.pop() {
            match entry {
                StackEntry::Join { bounding_box } => {
                    let second = nodes
                        .pop()
                        .expect("a second node should be on the node stack");
                    let first = nodes
                        .pop()
                        .expect("a first node should be on the node stack");
                    nodes.push(BvhNode {
                        bounding_box,
                        node_type: BvhNodeType::Node {
                            subtree_size: first.subtree_size() + second.subtree_size() + 2,
                            first: Box::new(first),
                            second: Box::new(second),
                        },
                    });
                }
                StackEntry::Split { begin, end, hull } => {
                    let items = &mut all_items[begin..end];

                    if items.len() == 1 {
                        nodes.push(BvhNode {
                            bounding_box: hull.aabbs,
                            node_type: BvhNodeType::Leaf { index: begin },
                        });
                        continue;
                    }

                    // Find the best axis for splitting, and check if the items are too close to each other to be
                    // able to find good clusters.
                    let (split_axis_width, split_axis) = hull.largest_axis();
                    if split_axis_width < DIST_EPSILON {
                        // Split the items evenly between the child nodes.
                        let middle = items.len() / 2;
                        let (first_items, second_items) = items.split_at(middle);
                        let mut first_hull = Hull::new(first_items[0].aabb());
                        for item in first_items {
                            first_hull = first_hull.expand(&item.aabb());
                        }
                        let mut second_hull = Hull::new(second_items[0].aabb());
                        for item in second_items {
                            second_hull = second_hull.expand(&item.aabb());
                        }

                        stack.push(StackEntry::Join {
                            bounding_box: hull.aabbs,
                        });
                        stack.push(StackEntry::Split {
                            begin: begin + middle,
                            end,
                            hull: second_hull,
                        });
                        stack.push(StackEntry::Split {
                            begin,
                            end: begin + middle,
                            hull: first_hull,
                        });
                    } else {
                        // Partition the axis into a number of evenly distributed buckets the items can fall into.
                        const BUCKETS: usize = 6;
                        let mut bucket_sizes: [usize; BUCKETS] = Default::default();
                        let mut bucket_hulls: [Option<Hull>; BUCKETS] = Default::default();

                        let min_bound = split_axis.point_element(hull.centroids.min);
                        bucket_indices.clear();
                        for item in &*items {
                            let bounding_box = item.aabb();
                            let position = split_axis.point_element(bounding_box.center());
                            let float_index =
                                BUCKETS as f32 * (position - min_bound) / split_axis_width;
                            let index = (float_index as usize).min(BUCKETS - 1);

                            bucket_indices.push(index);
                            bucket_sizes[index] += 1;

                            if let Some(hull) = &mut bucket_hulls[index] {
                                *hull = hull.expand(&bounding_box);
                            } else {
                                bucket_hulls[index] = Some(Hull::new(bounding_box));
                            }
                        }

                        // Find the most efficient split.
                        let mut min_cost = f32::INFINITY;
                        let mut min_cost_split = 0;
                        let hull_area = hull.aabbs.surface_area();
                        for index in 1..BUCKETS {
                            let (first_sizes, second_sizes) = bucket_sizes.split_at(index);
                            let (first_hulls, second_hulls) = bucket_hulls.split_at(index);
                            let (first_count, first_area) =
                                get_bucket_stats(first_sizes, first_hulls);
                            let (second_count, second_area) =
                                get_bucket_stats(second_sizes, second_hulls);

                            let cost = (first_area * first_count as f32
                                + second_area * second_count as f32)
                                / hull_area;

                            if cost < min_cost {
                                min_cost_split = index;
                                min_cost = cost;
                            }
                        }

                        let mut index = 0;
                        let mut new_index = items.len() - 1;
                        loop {
                            while bucket_indices[index] < min_cost_split {
                                index += 1;
                            }

                            while bucket_indices[new_index] >= min_cost_split {
                                new_index -= 1;
                            }

                            if index >= new_index {
                                break;
                            }

                            items.swap(index, new_index);
                            bucket_indices.swap(index, new_index);
                        }

                        let (first_sizes, second_sizes) = bucket_sizes.split_at(min_cost_split);
                        let (first_hulls, second_hulls) = bucket_hulls.split_at_mut(min_cost_split);
                        let (first_begin, first_end, first_hull) =
                            merge_buckets(begin, first_sizes, first_hulls);
                        let (second_begin, second_end, second_hull) =
                            merge_buckets(first_end, second_sizes, second_hulls);

                        stack.push(StackEntry::Join {
                            bounding_box: hull.aabbs,
                        });
                        stack.push(StackEntry::Split {
                            begin: second_begin,
                            end: second_end,
                            hull: second_hull.expect("there should be a second items hull"),
                        });
                        stack.push(StackEntry::Split {
                            begin: first_begin,
                            end: first_end,
                            hull: first_hull.expect("there should be a first items hull"),
                        });
                    }
                }
            }
        }

        Self {
            nodes: nodes
                .pop()
                .map_or_else(Vec::new, |node| node.flatten(all_items)),
        }
    }
}

impl<T> Bvh<T> {
    pub fn ray_intersect(&self, ray: Ray3<f32>) -> Intersections<T, Ray3<f32>> {
        Intersections {
            nodes: self.nodes.iter(),
            intersecting: ray,
        }
    }

    pub fn point_intersect(&self, point: Point3<f32>) -> Intersections<T, Point3<f32>> {
        Intersections {
            nodes: self.nodes.iter(),
            intersecting: point,
        }
    }
}

fn get_bucket_stats(sizes: &[usize], hulls: &[Option<Hull>]) -> (usize, f32) {
    let (count, aabb) = sizes.iter().zip(hulls).fold(
        (0, None),
        |(acc_size, acc_hull): (usize, Option<Aabb3<f32>>), (&size, hull)| {
            if let Some(hull) = hull {
                let aabb = acc_hull.map_or(hull.aabbs, |aabb| aabb.union(&hull.aabbs));
                (acc_size + size, Some(aabb))
            } else {
                assert_eq!(size, 0);
                (acc_size, acc_hull)
            }
        },
    );
    let area = aabb.as_ref().map_or(0.0, Aabb3::surface_area);

    (count, area)
}

fn merge_buckets(
    begin: usize,
    sizes: &[usize],
    hulls: &mut [Option<Hull>],
) -> (usize, usize, Option<Hull>) {
    sizes.iter().zip(hulls).fold(
        (begin, begin, None),
        |(acc_begin, acc_end, acc_hull), (&size, hull)| {
            if let Some(hull) = hull.take() {
                let hull = acc_hull.map_or_else(|| hull.clone(), |acc_hull| hull.join(&acc_hull));

                (acc_begin, acc_end + size, Some(hull))
            } else {
                assert_eq!(size, 0);
                (acc_begin, acc_end, acc_hull)
            }
        },
    )
}

pub struct Intersections<'a, T, I> {
    nodes: std::slice::Iter<'a, FlatBvhNode<T>>,
    intersecting: I,
}

impl<'a, T> Intersections<'a, T, Ray3<f32>> {
    pub fn next(&mut self, max_distance: f32) -> Option<&T> {
        // Contains the next node after skipping a few
        let mut next_node = None;

        while let Some(node) = next_node.take().or_else(|| self.nodes.next()) {
            if let Some(distance) = aabb_intersection_distance(node.bounding_box, self.intersecting)
            {
                if distance >= max_distance {
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

impl<'a, T> Iterator for Intersections<'a, T, Point3<f32>> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        // Contains the next node after skipping a few
        let mut next_node = None;

        while let Some(node) = next_node.take().or_else(|| self.nodes.next()) {
            if node.bounding_box.contains(&self.intersecting) {
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

enum StackEntry {
    Split {
        begin: usize,
        end: usize,
        hull: Hull,
    },
    Join {
        bounding_box: Aabb3<f32>,
    },
}

struct BvhNode {
    bounding_box: Aabb3<f32>,
    node_type: BvhNodeType,
}

impl BvhNode {
    fn subtree_size(&self) -> usize {
        match self.node_type {
            BvhNodeType::Node { subtree_size, .. } => subtree_size,
            BvhNodeType::Leaf { .. } => 0,
        }
    }

    fn flatten<T>(self, values: Vec<T>) -> Vec<FlatBvhNode<T>> {
        let mut values = values.into_iter();
        let mut flat_nodes = Vec::new();
        let mut stack = vec![self];

        let mut next_index = 0;

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
                BvhNodeType::Leaf { index } => {
                    assert_eq!(index, next_index);
                    next_index += 1;
                    FlatBvhNodeType::Leaf {
                        item: values.next().unwrap(),
                    }
                }
            };

            flat_nodes.push(FlatBvhNode {
                bounding_box: node.bounding_box,
                node_type,
            });
        }

        flat_nodes
    }
}

enum BvhNodeType {
    Node {
        first: Box<BvhNode>,
        second: Box<BvhNode>,
        subtree_size: usize,
    },
    Leaf {
        index: usize,
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
