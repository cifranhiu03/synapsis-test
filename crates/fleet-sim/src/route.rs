//! Route polylines + GPS interpolation for the simulator.
//!
//! Coordinates are loosely placed near an open-pit area in East Kalimantan
//! so the dashboard shows a recognisable region. Latitude/longitude are
//! treated as a small flat patch — at this scale and for a demo the
//! great-circle correction isn't worth the complexity.

use fleet_proto::v1::GpsFix;
use rand::Rng;

/// A truck route — load point at index 0, crusher at the last index.
/// `Hauling` traverses 0 → end; `Returning` traverses end → 0.
#[derive(Clone, Debug)]
pub struct Route {
    pub points: Vec<(f64, f64)>,
    /// Cached cumulative distances in metres along the polyline.
    pub cum_m: Vec<f64>,
}

impl Route {
    pub fn new(points: Vec<(f64, f64)>) -> Self {
        let mut cum = Vec::with_capacity(points.len());
        cum.push(0.0);
        for w in points.windows(2) {
            let d = haversine_m(w[0], w[1]);
            cum.push(cum.last().unwrap() + d);
        }
        Self { points, cum_m: cum }
    }

    pub fn total_m(&self) -> f64 {
        *self.cum_m.last().unwrap_or(&0.0)
    }

    /// Position at fraction `t` ∈ [0, 1] along the route.
    pub fn position_at(&self, t: f64) -> (f64, f64) {
        let t = t.clamp(0.0, 1.0);
        let target = t * self.total_m();
        // Find segment whose cumulative distance bracket includes target.
        let i = match self
            .cum_m
            .binary_search_by(|d| d.partial_cmp(&target).unwrap_or(std::cmp::Ordering::Equal))
        {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1).min(self.points.len() - 2),
        };
        let i = i.min(self.points.len() - 2);
        let seg_len = (self.cum_m[i + 1] - self.cum_m[i]).max(1e-9);
        let local = ((target - self.cum_m[i]) / seg_len).clamp(0.0, 1.0);
        let (a_lat, a_lon) = self.points[i];
        let (b_lat, b_lon) = self.points[i + 1];
        (a_lat + (b_lat - a_lat) * local, a_lon + (b_lon - a_lon) * local)
    }
}

/// Haversine distance in metres (good enough at sub-km scale).
fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    const R: f64 = 6_371_000.0;
    let (lat1, lon1) = (a.0.to_radians(), a.1.to_radians());
    let (lat2, lon2) = (b.0.to_radians(), b.1.to_radians());
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * R * h.sqrt().asin()
}

/// Five distinct routes around the same pit. Same load area, different
/// haul paths and crusher destinations so the dashboard reads as a real
/// fleet rather than five copies of one truck.
pub fn fleet_routes() -> Vec<Route> {
    // Anchor near Sangatta, East Kalimantan. Small offsets in degrees:
    // 0.001° ≈ 110 m.
    let load = (0.5380, 117.5620);
    let mk = |waypoints: &[(f64, f64)], crusher: (f64, f64)| -> Route {
        let mut pts = vec![load];
        pts.extend_from_slice(waypoints);
        pts.push(crusher);
        Route::new(pts)
    };
    vec![
        mk(
            &[(0.5395, 117.5640), (0.5410, 117.5670), (0.5420, 117.5700)],
            (0.5430, 117.5740),
        ),
        mk(
            &[(0.5375, 117.5605), (0.5360, 117.5580), (0.5345, 117.5550)],
            (0.5330, 117.5510),
        ),
        mk(
            &[(0.5390, 117.5615), (0.5405, 117.5605), (0.5425, 117.5595)],
            (0.5450, 117.5585),
        ),
        mk(
            &[(0.5378, 117.5635), (0.5365, 117.5660), (0.5350, 117.5685)],
            (0.5335, 117.5715),
        ),
        mk(
            &[(0.5388, 117.5640), (0.5400, 117.5660), (0.5420, 117.5650)],
            (0.5440, 117.5630),
        ),
    ]
}

/// Sample a noisy GPS fix from a true position. Roughly ±2 m horizontal
/// noise modelled as uniform on a square — the difference between uniform
/// and gaussian is irrelevant at this fidelity.
pub fn noisy_fix<R: Rng + ?Sized>(true_pos: (f64, f64), rng: &mut R) -> GpsFix {
    // 1 deg latitude ≈ 111 km, so 2 m ≈ 1.8e-5°.
    let jitter_deg = 1.8e-5;
    let lat = true_pos.0 + rng.gen_range(-jitter_deg..=jitter_deg);
    let lon = true_pos.1 + rng.gen_range(-jitter_deg..=jitter_deg);
    GpsFix {
        lat,
        lon,
        alt_m: 120.0 + rng.gen_range(-5.0..=5.0),
        hdop: rng.gen_range(0.7..=1.4),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_at_endpoints_match_route() {
        let r = Route::new(vec![(0.0, 0.0), (0.0, 0.001), (0.0, 0.002)]);
        let start = r.position_at(0.0);
        let end = r.position_at(1.0);
        assert!((start.0 - 0.0).abs() < 1e-9 && (start.1 - 0.0).abs() < 1e-9);
        assert!((end.0 - 0.0).abs() < 1e-9 && (end.1 - 0.002).abs() < 1e-9);
    }

    #[test]
    fn position_at_midpoint_is_between() {
        let r = Route::new(vec![(0.0, 0.0), (0.0, 0.002)]);
        let mid = r.position_at(0.5);
        assert!((mid.1 - 0.001).abs() < 1e-9, "got {mid:?}");
    }

    #[test]
    fn fleet_has_five_distinct_routes() {
        let routes = fleet_routes();
        assert_eq!(routes.len(), 5);
        // Crushers are distinct.
        let crushers: std::collections::HashSet<_> =
            routes.iter().map(|r| format!("{:?}", r.points.last().unwrap())).collect();
        assert_eq!(crushers.len(), 5);
    }
}
