//! Native aperture visibility between physical door faces.
use serde_json::Value;

type Point = [f64; 3];
fn point(value: &Value) -> Option<Point> {
    let p = [value[0].as_f64()?, value[1].as_f64()?, value[2].as_f64()?];
    p.iter().all(|x| x.is_finite()).then_some(p)
}
fn sub(a: Point, b: Point) -> Point {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: Point, b: Point) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}

pub(super) fn exit_links(door: &Value, other: &Value) -> [[f64; 2]; 2] {
    let mut out = [[0.0; 2]; 2];
    if door["world"] != other["world"] || door["id"] == other["id"] {
        return out;
    }
    let known = door["visibility_unknown"].as_bool() == Some(false);
    if known
        && door["visible_doors"]
            .as_array()
            .is_some_and(|ids| !ids.contains(&other["id"]))
    {
        return out;
    }
    let unknown = || {
        std::array::from_fn(|side| {
            if side == 1 && door["two_sided"] == false {
                [0.0; 2]
            } else if other["two_sided"] == false {
                [1.0, 0.0]
            } else {
                [0.5, 0.5]
            }
        })
    };
    let geometry = point(&door["base"])
        .zip(point(&door["normal"]))
        .zip(point(&other["base"]).zip(point(&other["normal"])));
    let Some(((base, normal), (other_base, other_normal))) = geometry else {
        return unknown();
    };
    let length = dot(normal, normal).sqrt();
    if length < 1e-6 {
        return unknown();
    }
    let normal = normal.map(|v| v / length);
    let tangent = [-normal[1], normal[0], 0.0];
    let width = super::size(door, "width");
    let height = super::size(door, "height");
    for side in 0..2 {
        if side == 1 && door["two_sided"] == false {
            continue;
        }
        let aperture = Aperture {
            base,
            normal: normal.map(|v| if side == 0 { v } else { -v }),
            tangent,
            width,
            height,
        };
        for distance in [16.0, 32.0, 64.0] {
            let eye = aperture.eye_at(distance);
            if aperture.sees(other, eye, distance) {
                let entered_side = usize::from(dot(sub(eye, other_base), other_normal) <= 0.0);
                out[side][entered_side] += 1.0 / 3.0;
            }
        }
    }
    out
}

struct Aperture {
    base: Point,
    normal: Point,
    tangent: Point,
    width: f64,
    height: f64,
}

impl Aperture {
    fn eye_at(&self, distance: f64) -> Point {
        let mut eye = std::array::from_fn(|i| self.base[i] - self.normal[i] * distance);
        eye[2] += 60.0f64.min(self.height - 8.0);
        eye
    }

    fn sees(&self, other: &Value, eye: Point, distance: f64) -> bool {
        let &Self {
            base,
            normal,
            tangent,
            width,
            height,
        } = self;
        let (Some(origin), Some(other_normal)) = (point(&other["base"]), point(&other["normal"]))
        else {
            return true;
        };
        if other["two_sided"] == false && dot(sub(eye, origin), other_normal) <= 0.0 {
            return false;
        }
        let across = [-other_normal[1], other_normal[0], 0.0];
        let corners: [Point; 4] = std::array::from_fn(|i| {
            let u = if i < 2 { -0.5 } else { 0.5 };
            let z = if i % 2 == 0 {
                0.0
            } else {
                super::size(other, "height")
            };
            let mut p =
                std::array::from_fn(|i| origin[i] + across[i] * u * super::size(other, "width"));
            p[2] += z;
            p
        });
        let forward = corners.map(|p| dot(sub(p, eye), normal));
        if forward.iter().all(|d| *d <= distance) {
            return false;
        }
        if forward.iter().any(|d| *d <= distance) {
            return true;
        }
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for (p, forward) in corners.into_iter().zip(forward) {
            let at =
                std::array::from_fn(|i| eye[i] + (p[i] - eye[i]) * distance / forward - base[i]);
            let projected = [dot(at, tangent), at[2]];
            for i in 0..2 {
                lo[i] = lo[i].min(projected[i]);
                hi[i] = hi[i].max(projected[i]);
            }
        }
        hi[0] >= -width * 0.5 && lo[0] <= width * 0.5 && hi[1] >= 0.0 && lo[1] <= height
    }
}
