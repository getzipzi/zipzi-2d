//! Broad-phase component and segment queries. Exact geo predicates decide topology;
//! disjoint bounds never need point, polygon-pair or segment-pair tests.
use super::limit;
use crate::SdkError;
use geo::{
    coordinate_position::CoordPos, dimensions::Dimensions, BoundingRect, Contains, Intersects,
    MultiPolygon, Point, Polygon, Relate, RemoveRepeatedPoints, Validation,
};
use rstar::{
    primitives::{GeomWithData, Rectangle},
    RTree, RTreeObject, AABB,
};

type Entry = GeomWithData<Rectangle<[f64; 2]>, usize>;

pub(super) struct PolygonIndex<'a> {
    polygons: &'a MultiPolygon<f64>,
    tree: RTree<Entry>,
}

impl<'a> PolygonIndex<'a> {
    pub fn new(polygons: &'a MultiPolygon<f64>) -> Self {
        let entries = polygons
            .0
            .iter()
            .enumerate()
            .filter_map(|(id, polygon)| {
                let bounds = polygon.bounding_rect()?;
                Some(GeomWithData::new(
                    Rectangle::from_corners(
                        [bounds.min().x, bounds.min().y],
                        [bounds.max().x, bounds.max().y],
                    ),
                    id,
                ))
            })
            .collect();
        Self {
            polygons,
            tree: RTree::bulk_load(entries),
        }
    }

    pub fn contains(&self, point: &Point<f64>) -> bool {
        self.tree
            .locate_in_envelope_intersecting(&AABB::from_point([point.x(), point.y()]))
            .any(|entry| self.polygons.0[entry.data].contains(point))
    }

    fn valid_pairs(&self, mut remaining: usize) -> Result<bool, SdkError> {
        for entry in &self.tree {
            for other in self.tree.locate_in_envelope_intersecting(&entry.envelope()) {
                if other.data <= entry.data {
                    continue;
                }
                remaining = remaining.checked_sub(1).ok_or_else(limit)?;
                let relation = self.polygons.0[entry.data].relate(&self.polygons.0[other.data]);
                // Match geo's MultiPolygon validity rules: point contacts are
                // allowed, overlapping interiors and shared line segments aren't.
                if relation.get(CoordPos::Inside, CoordPos::Inside) == Dimensions::TwoDimensional
                    || relation.get(CoordPos::OnBoundary, CoordPos::OnBoundary)
                        == Dimensions::OneDimensional
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

pub(super) fn is_valid(polygons: &MultiPolygon<f64>) -> Result<bool, SdkError> {
    // Validate rings before constructing bounds or running inter-polygon relate.
    if polygons.0.iter().any(|polygon| !polygon_valid(polygon)) {
        return Ok(false);
    }
    PolygonIndex::new(polygons).valid_pairs(1_048_576)
}

fn polygon_valid(polygon: &Polygon<f64>) -> bool {
    let ring = polygon.exterior();
    // Keep geo's complete hole/exterior relationship checks. Simple short
    // rings are also cheaper to validate without building an index.
    if !polygon.interiors().is_empty() || ring.0.len() < 32 {
        return polygon.is_valid();
    }
    if ring.0.iter().any(|p| !p.x.is_finite() || !p.y.is_finite())
        || ring.remove_repeated_points().0.len() < 4
    {
        return false;
    }
    // Match geo 0.33's ring self-intersection predicate exactly, but only
    // compare segment pairs with intersecting bounds. Raster offsets often
    // have thousands of edges; the all-pairs check dominated their runtime.
    let lines: Vec<_> = ring.lines().collect();
    let tree = RTree::bulk_load(
        lines
            .iter()
            .enumerate()
            .map(|(id, line)| {
                GeomWithData::new(
                    Rectangle::from_corners(
                        [line.start.x.min(line.end.x), line.start.y.min(line.end.y)],
                        [line.start.x.max(line.end.x), line.start.y.max(line.end.y)],
                    ),
                    id,
                )
            })
            .collect(),
    );
    for entry in &tree {
        let line = &lines[entry.data];
        for other in tree.locate_in_envelope_intersecting(&entry.envelope()) {
            if other.data <= entry.data {
                continue;
            }
            let other = &lines[other.data];
            if line.intersects(other) && line.start != other.end && line.end != other.start {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{LineString, Polygon, Rect};

    fn square(x: f64, y: f64, size: f64) -> Polygon<f64> {
        Rect::new((x, y), (x + size, y + size)).to_polygon()
    }

    #[test]
    fn indexed_predicates_keep_holes_contacts_and_invalidity() {
        let holed = Polygon::new(
            square(0., 0., 10.).exterior().clone(),
            vec![square(2., 2., 6.).exterior().clone()],
        );
        for polygons in [
            vec![],
            vec![square(0., 0., 1.), square(1., 1., 1.)],
            vec![square(0., 0., 1.), square(1., 0., 1.)],
            vec![square(0., 0., 2.), square(1., 1., 2.)],
            vec![square(0., 0., 10.), square(3., 3., 1.)],
            vec![holed.clone(), square(3., 3., 1.)],
            vec![holed, square(1., 3., 2.)],
            vec![Polygon::new(
                LineString::from(vec![(0., 0.), (2., 2.), (0., 2.), (2., 0.), (0., 0.)]),
                vec![],
            )],
        ] {
            let polygons = MultiPolygon(polygons);
            assert_eq!(is_valid(&polygons).unwrap(), polygons.is_valid());
            if !polygons.is_valid() {
                continue;
            }
            let index = PolygonIndex::new(&polygons);
            for x in -2..=22 {
                for y in -2..=22 {
                    let point = Point::new(x as f64 * 0.5, y as f64 * 0.5);
                    assert_eq!(index.contains(&point), polygons.contains(&point));
                }
            }
        }
    }

    #[test]
    fn disjoint_islands_need_no_pair_predicates() {
        let polygons = MultiPolygon(
            (0..16_384)
                .map(|i| square((i % 128 * 4) as f64, (i / 128 * 4) as f64, 1.))
                .collect(),
        );
        assert!(PolygonIndex::new(&polygons).valid_pairs(0).unwrap());
    }

    #[test]
    fn overlapping_bounds_respect_validation_budget() {
        let polygons = MultiPolygon(vec![square(0., 0., 2.), square(1., 1., 2.)]);
        assert_eq!(
            PolygonIndex::new(&polygons)
                .valid_pairs(0)
                .unwrap_err()
                .code
                .as_ref(),
            "ALPHA_MESH_LIMIT"
        );
    }

    #[test]
    fn indexed_ring_validation_matches_geo_for_large_and_degenerate_rings() {
        let circle: Vec<_> = (0..64)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / 64.0;
                (angle.cos(), angle.sin())
            })
            .collect();
        let mut cases = vec![circle.clone()];
        let mut crossed = circle.clone();
        crossed.swap(10, 40);
        cases.push(crossed);
        let mut repeated = circle.clone();
        repeated.splice(8..8, [circle[7]; 4]);
        cases.push(repeated);
        let mut spike = circle.clone();
        spike.splice(8..8, [(0.0, 0.0), circle[7]]);
        cases.push(spike);
        cases.push(vec![(0.0, 0.0); 64]);
        cases.push((0..64).map(|i| (i as f64, 0.0)).collect());
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut nonfinite = circle.clone();
            nonfinite[20].0 = value;
            cases.push(nonfinite);
        }
        // Seeded arbitrary walks exercise intersections and repeated endpoints.
        let mut state = 0x532e_a90d_u32;
        for _ in 0..128 {
            cases.push(
                (0..48)
                    .map(|_| {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        ((state % 16) as f64, ((state >> 8) % 16) as f64)
                    })
                    .collect(),
            );
        }
        for points in cases {
            let polygon = Polygon::new(LineString::from(points), vec![]);
            assert_eq!(polygon_valid(&polygon), polygon.is_valid(), "{polygon:?}");
        }
    }
}
