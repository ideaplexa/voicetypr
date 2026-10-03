//! Canvas geometry preserves the old 260×64 pill's visible edge (6px inset).
// Transparent margin lets the 26px peek button grow around the 12px dot
// without clipping at canvas edges. canvas_origin preserves its screen position.
const QUICK_MARGIN: f64 = 11.0;
pub const CANVAS_WIDTH: f64 = 440.0 + QUICK_MARGIN * 2.0;
pub const CANVAS_HEIGHT: f64 = 420.0 + QUICK_MARGIN * 2.0;
pub const LEGACY_WIDTH: f64 = 260.0;
pub const LEGACY_HEIGHT: f64 = 64.0;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Geometry {
    pub anchor: String,
    pub anchor_x: f64,
    pub anchor_y: f64,
}
impl Geometry {
    pub fn new(position: &str) -> Self {
        let position = match position {
            "top-left" | "top-center" | "top-right" | "bottom-left" | "bottom-center"
            | "bottom-right" => position,
            _ => "bottom-center",
        };
        Self {
            anchor: position.to_owned(),
            anchor_x: if position.ends_with("-left") {
                QUICK_MARGIN
            } else if position.ends_with("-right") {
                CANVAS_WIDTH - QUICK_MARGIN
            } else {
                CANVAS_WIDTH / 2.0
            },
            anchor_y: if position.starts_with("top-") {
                6.0 + QUICK_MARGIN
            } else {
                CANVAS_HEIGHT - 6.0 - QUICK_MARGIN
            },
        }
    }
    pub fn canvas_origin(&self, legacy_origin: (f64, f64)) -> (f64, f64) {
        let old_x = if self.anchor.ends_with("-left") {
            0.0
        } else if self.anchor.ends_with("-right") {
            LEGACY_WIDTH
        } else {
            LEGACY_WIDTH / 2.0
        };
        let old_y = if self.anchor.starts_with("top-") {
            6.0
        } else {
            LEGACY_HEIGHT - 6.0
        };
        (
            legacy_origin.0 + old_x - self.anchor_x,
            legacy_origin.1 + old_y - self.anchor_y,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn peek_record_fits_around_the_dot_on_every_canvas_edge() {
        for position in [
            "top-left",
            "top-center",
            "top-right",
            "bottom-left",
            "bottom-center",
            "bottom-right",
        ] {
            let g = Geometry::new(position);
            let x = g.anchor_x
                + if position.ends_with("-left") {
                    6.0
                } else if position.ends_with("-right") {
                    -6.0
                } else {
                    0.0
                };
            let y = g.anchor_y
                + if position.starts_with("top-") {
                    6.0
                } else {
                    -6.0
                };
            assert!(x - 13.0 >= 0.0 && x + 13.0 <= CANVAS_WIDTH);
            assert!(y - 13.0 >= 0.0 && y + 13.0 <= CANVAS_HEIGHT);
        }
    }
    #[test]
    fn all_anchors_preserve_legacy_screen_coordinates_at_both_scales() {
        for scale in [1.0, 2.0] {
            for vertical in ["top", "bottom"] {
                for horizontal in ["left", "center", "right"] {
                    let position = format!("{vertical}-{horizontal}");
                    // Nonzero/negative monitor origin, offset in logical pixels.
                    let (x, y, w, h, offset) = (
                        -1920.0 / scale,
                        120.0 / scale,
                        1920.0 / scale,
                        1080.0 / scale,
                        23.0,
                    );
                    let left = match horizontal {
                        "left" => x + offset,
                        "right" => x + w - LEGACY_WIDTH - offset,
                        _ => x + (w - LEGACY_WIDTH) / 2.0,
                    };
                    let top = if vertical == "top" {
                        y + offset
                    } else {
                        y + h - LEGACY_HEIGHT - offset
                    };
                    let g = Geometry::new(&position);
                    let origin = g.canvas_origin((left, top));
                    let expected_x = match horizontal {
                        "left" => left,
                        "right" => left + LEGACY_WIDTH,
                        _ => left + LEGACY_WIDTH / 2.0,
                    };
                    let expected_y = top + if vertical == "top" { 6.0 } else { 58.0 };
                    assert_eq!((origin.0 + g.anchor_x) * scale, expected_x * scale);
                    assert_eq!((origin.1 + g.anchor_y) * scale, expected_y * scale);
                }
            }
        }
    }
}
