# This script does some useful transformations to the Tiled JSON format
# to make it easier to parse, strip unnecessary data,
# and to match Starframe's coordinate system.
# Usage documented in `justfile`

# angles to counterclockwise radians
def angle_rad: 0.0174532925 * .rotation;
def flip_rotation: 6.28318531 - angle_rad;
def x: .x;
# Y axis up
def y: -.y;
def hw: .width / 2.0;
def hh: .height / 2.0;
# object origins to their centers
def x_center: (.x + hw * (angle_rad | cos) - hh * (angle_rad | sin));
def y_center: -(.y + hw * (angle_rad | sin) + hh * (angle_rad | cos));
# scaling factor so we can use Tiled at a comfortable zoom level
def scale(f): f / 30;

{
  recipes: [.layers[] | .objects[] | {
      type,
      pose: {
        x: scale(x_center),
        y: scale(y_center),
        rotation: flip_rotation,
      },
      width: scale(.width),
      height: scale(.height),
      polyline: (if .polyline != null then
        [.polyline[] | { x: scale(x), y: scale(y) }]
        else null end),
      ellipse: .ellipse?,
    }
    + if .properties != null then [.properties[] | { key: .name, value }] | from_entries else null end
  ]
}
| delpaths([path(.. | nulls)])
