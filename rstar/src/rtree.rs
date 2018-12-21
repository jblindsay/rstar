use crate::algorithm::bulk_load;
use crate::algorithm::iterators::*;
use crate::algorithm::nearest_neighbor;
use crate::algorithm::removal;
use crate::algorithm::selection_functions::*;
use crate::envelope::Envelope;
use crate::object::{PointDistance, RTreeObject};
use crate::params::{DefaultParams, InsertionStrategy, RTreeParams};
use crate::structures::node::ParentNodeData;
use crate::Point;

impl<T> Default for RTree<T>
where
    T: RTreeObject,
{
    fn default() -> Self {
        Self::new()
    }
}

/// An n-dimensional r-tree data structure.
///
/// # R-Trees
/// R-Trees are data structures containing multi-dimensional objects like points, rectangles
/// or polygons. They are optimized for retrieving the nearest neighbor at any point.
///
/// R-trees can efficiently find answers to queries like "Find the nearest point of a polygon",
/// "Find all police stations within a rectangle" or "Find the 10 nearest restaurants, sorted
/// by their distances". Compared to a naive implementation for these scenarios that runs
/// in `O(n)` for `n` inserted elements, r-trees reduce this time to `O(log(n))`.
///
/// However, creating an r-tree is time consuming
/// and runs in `O(n * log(n))`. Thus, r-trees are suited best if many queries and only few
/// insertions are made. Also, rstar supports [bulk loading](struct.RTree.html#method.bulk_load),
/// which cuts down the constant factors when creating an r-tree significantly compared to
/// sequential insertions.
///
/// R-trees are also _dynamic_, thus, points can be inserted and removed even if the tree
/// has been created before.
///
/// ## Partitioning heuristics
/// The inserted objects are internally partitioned into several boxes which should have small
/// overlap and volume. This is done heuristically. While the originally proposed heuristic focused
/// on fast insertion operations, the resulting r-trees were often suboptimally structured. Another
/// heuristic, called `R*-tree` (r-star-tree), was proposed to improve the tree structure at the cost of
/// longer insertion operations and is currently the crate's only implemented
/// [insertion strategy](trait.InsertionStrategy.html).
///
/// ## Further reading
/// For more information refer to the [wikipedia article](https://en.wikipedia.org/wiki/R-tree).
///
/// # Usage
/// The items inserted into an r-tree must implement the [RTreeObject](trait.RTreeObject.html)
/// trait. To support nearest neighbor queries, implement the [PointDistance](trait.PointDistance.html)
/// trait. Some useful geometric primitives that implement the above traits can be found in the
/// [primitives](mod.primitives.html) module.
///
/// ## Example
/// ```
/// use rstar::RTree;
///
/// let mut tree = RTree::new();
/// tree.insert([0.1, 0.0f32]);
/// tree.insert([0.2, 0.1]);
/// tree.insert([0.3, 0.0]);
///
/// assert_eq!(tree.nearest_neighbor(&[0.4, -0.1]), Some(&[0.3, 0.0]));
/// tree.remove(&[0.3, 0.0]);
/// assert_eq!(tree.nearest_neighbor(&[0.4, 0.3]), Some(&[0.2, 0.1]));
///
/// assert_eq!(tree.size(), 2);
/// // &RTree implements IntoIterator!
/// for point in &tree {
///     println!("Tree contains a point {:?}", point);
/// }
/// ```
///
/// ## Supported point types
/// All types implementing the [Point](trait.Point.html) trait can be used as underlying point type.
/// By default, fixed size arrays can be used as points.
///
/// ## Type Parameters
/// * `T`: The type of objects stored in the r-tree.
/// * `Params`: Compile time parameters that change the r-trees internal layout. Refer to the
/// [RTreeParams](trait.RTreeParams.html) trait for more information.
///
/// ## Defining methods generic over r-trees
/// If a library defines a method that should be generic over the r-tree type signature, make
/// sure to include both type parameters like this:
/// ```
/// # use rstar::{RTree,RTreeObject, RTreeParams};
/// pub fn generic_rtree_function<T, Params>(tree: &mut RTree<T, Params>)
/// where
///   T: RTreeObject,
///   Params: RTreeParams
/// {
///   // ...
/// }
/// ```
/// Otherwise, any user of `generic_rtree_function` would be forced to use
/// a tree with default parameters.
///
/// # Runtime and Performance
/// The runtime of query operations (e.g. `nearest neighbor` or `contains`) is usually
/// `O(log(n))`, where `n` refers to the number of elements contained in the r-tree.
/// A naive sequential algorithm would take `O(n)` time. However, r-trees incur higher
/// build up times: inserting an element into an r-tree costs `O(log(n))` time.
///
/// ## Bulk loading
/// In many scenarios, insertion is only done once for many points. In this case,
/// [bulk_load](#method.bulk_load) will be considerably faster. Its total run time
/// is still `O(log(n))`.
///
/// ## Element distribution
/// The tree's performance heavily relies on the spatial distribution of its elements.
/// Best performance is achieved if:
///  * No element is inserted more than once
///  * The overlapping area of elements should be as small a
///    possible.
///
/// For the edge case that all elements are overlapping (e.g, one and the same element
///  is `n` times), the performance of most operations usually degrades to `O(n)`.
#[derive(Clone)]
pub struct RTree<T, Params = DefaultParams>
where
    Params: RTreeParams,
    T: RTreeObject,
{
    root: ParentNodeData<T>,
    size: usize,
    _params: ::std::marker::PhantomData<Params>,
}

#[cfg(feature = "debug")]
#[doc(hidden)]
pub fn root<T, Params>(tree: &RTree<T, Params>) -> &ParentNodeData<T>
where
    T: RTreeObject,
    Params: RTreeParams,
{
    &tree.root
}

pub fn root_mut<T, Params>(tree: &mut RTree<T, Params>) -> &mut ParentNodeData<T>
where
    T: RTreeObject,
    Params: RTreeParams,
{
    &mut tree.root
}

struct DebugHelper<'a, T, Params>
where
    T: RTreeObject + ::std::fmt::Debug + 'a,
    Params: RTreeParams + 'a,
{
    rtree: &'a RTree<T, Params>,
}

impl<'a, T, Params> ::std::fmt::Debug for DebugHelper<'a, T, Params>
where
    T: RTreeObject + ::std::fmt::Debug,
    Params: RTreeParams,
{
    fn fmt(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        formatter.debug_set().entries(self.rtree.iter()).finish()
    }
}

impl<T, Params> ::std::fmt::Debug for RTree<T, Params>
where
    Params: RTreeParams,
    T: RTreeObject + ::std::fmt::Debug,
{
    fn fmt(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        formatter
            .debug_struct("RTree")
            .field("size", &self.size)
            .field("items", &DebugHelper { rtree: &self })
            .finish()
    }
}

impl<T> RTree<T>
where
    T: RTreeObject,
{
    /// Creates a new, empty r-tree.
    ///
    /// The created r-tree is configured with [default parameters](struct.DefaultParams.html).
    pub fn new() -> Self {
        Self::new_with_params()
    }
}

impl<T, Params> RTree<T, Params>
where
    Params: RTreeParams,
    T: RTreeObject,
{
    /// Creates a new, empty r-tree.
    ///
    /// The tree's compile time parameters must be specified. Refer to the
    /// [RTreeParams](trait.RTreeParams.html) trait for more information and a usage example.
    pub fn new_with_params() -> Self {
        RTree {
            root: ParentNodeData::new_root::<Params>(),
            size: 0,
            _params: Default::default(),
        }
    }

    /// Returns the number of objects in an r-tree.
    ///
    /// # Example
    /// ```
    /// use rstar::RTree;
    ///
    /// let mut tree = RTree::new();
    /// assert_eq!(tree.size(), 0);
    /// tree.insert([0.0, 1.0, 2.0]);
    /// assert_eq!(tree.size(), 1);
    /// tree.remove(&[0.0, 1.0, 2.0]);
    /// assert_eq!(tree.size(), 0);
    /// ```
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns an iterator over all elements contained in the tree.
    ///
    /// The order in which the elements are returned is not specified.
    ///
    /// # Example
    /// ```
    /// use rstar::RTree;
    /// let tree = RTree::bulk_load(&mut[[0.0, 0.1], [0.3, 0.2], [0.4, 0.2]]);
    /// for point in tree.iter() {
    ///     println!("This tree contains point {:?}", point);
    /// }
    /// ```
    pub fn iter(&self) -> RTreeIterator<T> {
        RTreeIterator::new(&self.root, SelectAllFunc)
    }

    /// Returns an iterator over all mutable elements contained in the tree.nearest_neighbor
    ///
    /// The order in which the elements are returned is not specified.nearest_neighbor
    ///
    /// *Note*: It is a logic error to change an inserted item's position or dimensions. This
    /// method is primarily meant for own implementations of [RTreeObject](trait.RTreeObject.html)
    /// which can contain arbitrary additional data.
    /// If the position or location of an inserted object need to change, you will need to [remove]
    /// and reinsert it.
    ///
    pub fn iter_mut(&mut self) -> RTreeIteratorMut<T> {
        RTreeIteratorMut::new(&mut self.root, SelectAllFunc)
    }

    /// Returns all elements contained in an [Envelope](trait.Envelope.html).
    ///
    /// Usually, an envelope is an [axis aligned bounding box](struct.AABB.html). This
    /// method can be used to get all elements that are fully contained within an envelope.
    ///
    /// # Example
    /// ```
    /// use rstar::{RTree, AABB};
    /// let mut tree = RTree::bulk_load(&mut [
    ///   [0.0, 0.0],
    ///   [0.0, 1.0],
    ///   [1.0, 1.0]
    /// ]);
    /// let half_unit_square = AABB::from_corners([0.0, 0.0], [0.5, 1.0]);
    /// let unit_square = AABB::from_corners([0.0, 0.0], [1.0, 1.0]);
    /// let elements_in_half_unit_square = tree.locate_in_envelope(&half_unit_square);
    /// let elements_in_unit_square = tree.locate_in_envelope(&unit_square);
    /// assert_eq!(elements_in_half_unit_square.count(), 2);
    /// assert_eq!(elements_in_unit_square.count(), 3);
    /// ```
    pub fn locate_in_envelope(&self, envelope: &T::Envelope) -> LocateInEnvelope<T> {
        LocateInEnvelope::new(&self.root, SelectInEnvelopeFunction::new(*envelope))
    }

    /// Mutable variant of [locate_in_envelope](#method.locate_in_envelope).
    pub fn locate_in_envelope_mut(&mut self, envelope: &T::Envelope) -> LocateInEnvelopeMut<T> {
        LocateInEnvelopeMut::new(&mut self.root, SelectInEnvelopeFunction::new(*envelope))
    }

    /// Returns all elements whose envelope intersects a given envelope.
    ///
    /// Usually, an envelope is an axis [aligned bounding box](struct.AABB.html).
    /// This method will return all elements whose AABB has some common area with
    /// a given AABB.
    pub fn locate_in_envelope_intersecting(
        &self,
        envelope: &T::Envelope,
    ) -> LocateInEnvelopeIntersecting<T> {
        LocateInEnvelopeIntersecting::new(
            &self.root,
            SelectInEnvelopeFuncIntersecting::new(*envelope),
        )
    }

    /// Mutable variant of [locate_in_envelope_intersecting](#method.locate_in_envelope_intersecting)
    pub fn locate_in_envelope_intersecting_mut(
        &mut self,
        envelope: &T::Envelope,
    ) -> LocateInEnvelopeIntersectingMut<T> {
        LocateInEnvelopeIntersectingMut::new(
            &mut self.root,
            SelectInEnvelopeFuncIntersecting::new(*envelope),
        )
    }
}

impl<T, Params> RTree<T, Params>
where
    Params: RTreeParams,
    T: PointDistance,
{
    /// Returns a single object that covers a given point.
    ///
    /// Method [contains_point](trait.PointDistance.html#method.contains_point)
    /// is used to determine if a tree element contains the given point.
    ///
    /// If multiple elements contain the given point, any of them is returned.
    pub fn locate_at_point(&self, point: &<T::Envelope as Envelope>::Point) -> Option<&T> {
        self.locate_all_at_point(point).next()
    }

    /// Mutable variant of [locate_at_point](#method.locate_at_point).
    pub fn locate_at_point_mut(
        &mut self,
        point: &<T::Envelope as Envelope>::Point,
    ) -> Option<&mut T> {
        self.locate_all_at_point_mut(point).next()
    }

    /// Locate all elements containing a given point.
    ///
    /// Method [contains_point](trait.PointDistance.html#method.contains_point) is used
    /// to determine if a tree element contains the given point.
    /// # Example
    /// ```
    /// use rstar::RTree;
    /// use rstar::primitives::Rectangle;
    ///
    /// let tree = RTree::bulk_load(&mut [
    ///   Rectangle::from_corners([0.0, 0.0], [2.0, 2.0]),
    ///   Rectangle::from_corners([1.0, 1.0], [3.0, 3.0])
    /// ]);
    ///
    /// assert_eq!(tree.locate_all_at_point(&[1.5, 1.5]).count(), 2);
    /// assert_eq!(tree.locate_all_at_point(&[0.0, 0.0]).count(), 1);
    /// assert_eq!(tree.locate_all_at_point(&[-1., 0.0]).count(), 0);
    /// ```
    pub fn locate_all_at_point(
        &self,
        point: &<T::Envelope as Envelope>::Point,
    ) -> LocateAllAtPoint<T> {
        LocateAllAtPoint::new(&self.root, SelectAtPointFunction::new(*point))
    }

    /// Mutable variant of [locate_at_point_mut](#method.locate_at_point_mut).
    pub fn locate_all_at_point_mut(
        &mut self,
        point: &<T::Envelope as Envelope>::Point,
    ) -> LocateAllAtPointMut<T> {
        LocateAllAtPointMut::new(&mut self.root, SelectAtPointFunction::new(*point))
    }

    /// Removes an element containing a given point.
    ///
    /// The removed element, if any, is returned. If multiple elements cover the given point,
    /// only one of them is removed and returned.
    ///
    /// # Example
    /// ```
    /// use rstar::RTree;
    /// use rstar::primitives::Rectangle;
    ///
    /// let mut tree = RTree::bulk_load(&mut [
    ///   Rectangle::from_corners([0.0, 0.0], [2.0, 2.0]),
    ///   Rectangle::from_corners([1.0, 1.0], [3.0, 3.0])
    /// ]);
    ///
    /// assert!(tree.remove_at_point(&[1.5, 1.5]).is_some());
    /// assert!(tree.remove_at_point(&[1.5, 1.5]).is_some());
    /// assert!(tree.remove_at_point(&[1.5, 1.5]).is_none());
    ///```
    pub fn remove_at_point(&mut self, point: &<T::Envelope as Envelope>::Point) -> Option<T> {
        let removal_function = SelectAtPointFunction::new(*point);
        let result = removal::remove::<_, Params, _>(&mut self.root, &removal_function);
        if result.is_some() {
            self.size -= 1;
        }
        result
    }
}

impl<T, Params> RTree<T, Params>
where
    Params: RTreeParams,
    T: RTreeObject + PartialEq,
{
    /// Returns `true` if a given element is equal (`==`) to an element in the
    /// r-tree.
    /// ```
    /// use rstar::RTree;
    ///
    /// let mut tree = RTree::new();
    /// assert!(!tree.contains(&[0.0, 2.0]));
    /// tree.insert([0.0, 2.0]);
    /// assert!(tree.contains(&[0.0, 2.0]));
    /// ```
    pub fn contains(&self, t: &T) -> bool {
        self.locate_in_envelope(&t.envelope()).any(|e| e == t)
    }

    /// Removes and returns an element of the r-tree equal (`==`) to a given element.
    ///
    /// If multiple elements equal to the given elements are contained in the tree, only
    /// one of them is removed and returned.
    /// # Example
    /// ```
    /// use rstar::RTree;
    ///
    /// let mut tree = RTree::new();
    /// tree.insert([0.0, 2.0]);
    /// // The element can be inserted twice just fine
    /// tree.insert([0.0, 2.0]);
    /// assert!(tree.remove(&[0.0, 2.0]).is_some());
    /// assert!(tree.remove(&[0.0, 2.0]).is_some());
    /// assert!(tree.remove(&[0.0, 2.0]).is_none());
    /// ```
    pub fn remove(&mut self, t: &T) -> Option<T> {
        let removal_function = SelectEqualsFunction::new(t);
        let result = removal::remove::<_, Params, _>(&mut self.root, &removal_function);
        if result.is_some() {
            self.size -= 1;
        }
        result
    }
}

impl<T, Params> RTree<T, Params>
where
    Params: RTreeParams,
    T: PointDistance,
{
    /// Returns the nearest neighbor for a given point.
    ///
    /// The distance is calculated by calling
    /// [PointDistance::distance_2](traits.PointDistance.html#method.distance_2)
    ///
    /// # Example
    /// ```
    /// use rstar::RTree;
    /// let tree = RTree::bulk_load(&mut [
    ///   [0.0, 0.0],
    ///   [0.0, 1.0],
    /// ]);
    /// assert_eq!(tree.nearest_neighbor(&[-1., 0.0]), Some(&[0.0, 0.0]));
    /// assert_eq!(tree.nearest_neighbor(&[0.0, 2.0]), Some(&[0.0, 1.0]));
    /// ```
    pub fn nearest_neighbor(&self, query_point: &<T::Envelope as Envelope>::Point) -> Option<&T> {
        if self.size > 0 {
            // The single-nearest-neighbor retrieval may in rare cases return None due to
            // rounding issues. The iterator will still work, though.
            nearest_neighbor::nearest_neighbor(&self.root, *query_point)
                .or_else(|| self.nearest_neighbor_iter(query_point).next())
        } else {
            None
        }
    }

    /// Returns all elements of the tree sorted by their distance to a given point.
    ///
    /// # Runtime
    /// Every `next()` call runs in `O(log(n))`. Creating the iterator runs in
    /// `O(log(n))`.
    /// The [r-tree documentation](struct.RTree.html) contains more information about
    /// r-tree performance.
    ///
    /// # Example
    /// ```
    /// use rstar::RTree;
    /// let tree = RTree::bulk_load(&mut [
    ///   [0.0, 0.0],
    ///   [0.0, 1.0],
    /// ]);
    ///
    /// let nearest_neighbors = tree.nearest_neighbor_iter(&[0.5, 0.0]).collect::<Vec<_>>();
    /// assert_eq!(nearest_neighbors, vec![&[0.0, 0.0], &[0.0, 1.0]]);
    /// ```
    pub fn nearest_neighbor_iter(
        &self,
        query_point: &<T::Envelope as Envelope>::Point,
    ) -> impl Iterator<Item = &T> {
        nearest_neighbor::NearestNeighborIterator::new(&self.root, *query_point)
    }
}

impl<T, Params> RTree<T, Params>
where
    T: RTreeObject,
    Params: RTreeParams,
{
    /// Inserts a new element into the r-tree.
    ///
    /// If the element has already been present in the tree, it will now be present twice.
    ///
    /// # Runtime
    /// This method runs in `O(log(n))`.
    /// The [r-tree documentation](struct.RTree.html) contains more information about
    /// r-tree performance.
    pub fn insert(&mut self, t: T) {
        Params::DefaultInsertionStrategy::insert(self, t);
        self.size += 1;
    }
}

impl<T, Params> RTree<T, Params>
where
    T: RTreeObject + Clone,
    <T::Envelope as Envelope>::Point: Point,
    Params: RTreeParams,
{
    /// Creates a new r-tree with some given elements and configurable parameters.
    ///
    /// For more information refer to [bulk_load_with_params](#methods.bulk_load_with_params)
    /// and [RTreeParameters](traits.RTreeParameters.html).
    pub fn bulk_load_with_params(elements: &mut [T]) -> Self {
        let root = bulk_load::bulk_load_with_params::<_, Params>(elements);
        RTree {
            root,
            size: elements.len(),
            _params: Default::default(),
        }
    }
}

impl<'a, T, Params> IntoIterator for &'a RTree<T, Params>
where
    T: RTreeObject,
    Params: RTreeParams,
{
    type IntoIter = RTreeIterator<'a, T>;
    type Item = &'a T;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, Params> IntoIterator for &'a mut RTree<T, Params>
where
    T: RTreeObject,
    Params: RTreeParams,
{
    type IntoIter = RTreeIteratorMut<'a, T>;
    type Item = &'a mut T;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T> RTree<T>
where
    T: RTreeObject + Clone,
    <T::Envelope as Envelope>::Point: Point,
{
    /// Creates a new r-tree with some elements already inserted.
    ///
    /// This method should be the preferred way for creating r-trees. It both
    /// runs faster and yields an r-tree with better internal structure that
    /// improves query performance.
    ///
    /// # Runtime
    /// Bulk loading runs in `O(n * log(n))`, where `n` is the number of loaded
    /// elements.
    pub fn bulk_load(elements: &mut [T]) -> Self {
        Self::bulk_load_with_params(elements)
    }
}

#[cfg(test)]
mod test {
    use super::RTree;
    use crate::algorithm::rstar::RStarInsertionStrategy;
    use crate::params::RTreeParams;
    use crate::test_utilities::{create_random_points, SEED_1};

    struct TestParams;
    impl RTreeParams for TestParams {
        const MIN_SIZE: usize = 10;
        const MAX_SIZE: usize = 20;
        type DefaultInsertionStrategy = RStarInsertionStrategy;
    }

    #[test]
    fn test_create_rtree_with_parameters() {
        let tree: RTree<[f32; 2], TestParams> = RTree::new_with_params();
        assert_eq!(tree.size(), 0);
    }

    #[test]
    fn test_insert_single() {
        let mut tree: RTree<_> = RTree::new();
        tree.insert([0.02f32, 0.4f32]);
        assert_eq!(tree.size(), 1);
        assert!(tree.contains(&[0.02, 0.4]));
        assert!(!tree.contains(&[0.3, 0.2]));
    }

    #[test]
    fn test_insert_many() {
        const NUM_POINTS: usize = 1000;
        let points = create_random_points(NUM_POINTS, SEED_1);
        let mut tree = RTree::new();
        for p in &points {
            tree.insert(*p);
        }
        assert_eq!(tree.size(), NUM_POINTS);
        for p in &points {
            assert!(tree.contains(p));
        }
    }

    #[test]
    fn test_fmt_debug() {
        let tree = RTree::bulk_load(&mut [[0, 1], [0, 1]]);
        let debug: String = format!("{:?}", tree);
        assert_eq!(debug, "RTree { size: 2, items: {[0, 1], [0, 1]} }");
    }
}