//! Where the bar sits on the screen, from the configured position.

use smithay_client_toolkit::shell::wlr_layer::Anchor;

use crate::core::{HorizontalAnchor, Position, VerticalAnchor};

/// The anchors and margins to give the layer surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// The screen edges the surface is anchored to. No horizontal anchor means
    /// centred, no vertical anchor means middle of the screen.
    pub anchor: Anchor,
    /// Margins as `(top, right, bottom, left)`, in pixels.
    pub margin: (i32, i32, i32, i32),
}

/// Compute the placement. The margin applies to every anchored edge. The surface
/// does not reserve space (exclusive zone 0), so the compositor keeps it clear of
/// the panel and the dock.
pub fn placement(position: &Position) -> Placement {
    let margin = i32::try_from(position.margin).unwrap_or(i32::MAX);
    let mut anchor = Anchor::empty();
    let (mut top, mut right, mut bottom, mut left) = (0, 0, 0, 0);
    match position.horizontal {
        HorizontalAnchor::Center => {}
        HorizontalAnchor::Left => {
            anchor |= Anchor::LEFT;
            left = margin;
        }
        HorizontalAnchor::Right => {
            anchor |= Anchor::RIGHT;
            right = margin;
        }
    }
    match position.vertical {
        VerticalAnchor::Center => {}
        VerticalAnchor::Top => {
            anchor |= Anchor::TOP;
            top = margin;
        }
        VerticalAnchor::Bottom => {
            anchor |= Anchor::BOTTOM;
            bottom = margin;
        }
    }
    Placement {
        anchor,
        margin: (top, right, bottom, left),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_position_is_bottom_centre_above_the_dock() {
        let placed = placement(&Position::default());
        assert_eq!(placed.anchor, Anchor::BOTTOM);
        assert_eq!(placed.margin, (0, 0, 28, 0));
    }

    #[test]
    fn corners_anchor_both_edges_with_the_margin() {
        let position = Position {
            horizontal: HorizontalAnchor::Right,
            vertical: VerticalAnchor::Top,
            margin: 10,
        };
        let placed = placement(&position);
        assert_eq!(placed.anchor, Anchor::TOP | Anchor::RIGHT);
        assert_eq!(placed.margin, (10, 10, 0, 0));
    }

    #[test]
    fn centre_on_both_axes_has_no_anchor() {
        let position = Position {
            horizontal: HorizontalAnchor::Center,
            vertical: VerticalAnchor::Center,
            margin: 50,
        };
        let placed = placement(&position);
        assert_eq!(placed.anchor, Anchor::empty());
        assert_eq!(placed.margin, (0, 0, 0, 0));
    }
}
