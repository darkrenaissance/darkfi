"""Python reimplementation of the shape-building routines from
src/ui/vector_art/shape.rs.

Coordinates are expr source strings ("w/2", "h - 10") or numbers (normalized
to float literals), matching the wire format of Api.set_property_shape: the
app compiles and evaluates them server-side, so there is no client-side
eval here. scaled()/offset() wrap coordinates in arithmetic just like the
app-side op surgery.
"""

import math

def _coord(x):
    if isinstance(x, str):
        return x
    x = float(x)
    neg = math.copysign(1.0, x) < 0
    if neg:
        x = -x
    s = repr(x)
    if "e" in s or "E" in s:
        # The app-side expr tokenizer does not accept scientific
        # notation (glow trig produces values like 6.1e-17), so render
        # tiny/huge floats as plain decimals.
        s = f"{x:.30f}".rstrip("0")
        if s.endswith("."):
            s += "0"
    # The tokenizer also has no unary minus, so negative constants
    # (outline borders, glow trig) are rendered as subtraction.
    return f"(0 - {s})" if neg else s

def _mul(a, b):
    return f"({_coord(a)} * {_coord(b)})"

def _add(a, b):
    return f"({_coord(a)} + {_coord(b)})"

class VectorShape:

    def __init__(self):
        # [x_expr, y_expr, [r, g, b, a]]
        self.verts = []
        self.indices = []

    def _vertex(self, x, y, color):
        self.verts.append([_coord(x), _coord(y), list(color)])

    def set(self, api, node_path, prop_name="shape", i=0):
        api.set_property_shape(node_path, prop_name, i, self.verts, self.indices)

    def join(self, other):
        off = len(self.verts)
        self.verts.extend([list(v) for v in other.verts])
        self.indices.extend([index + off for index in other.indices])

    def add_filled_box(self, x1, y1, x2, y2, color):
        self.add_gradient_box(x1, y1, x2, y2, [color, color, color, color])

    # Colors go clockwise from top-left
    def add_gradient_box(self, x1, y1, x2, y2, color):
        color = [list(c) for c in color]
        base = len(self.verts)
        self._vertex(x1, y1, color[0])
        self._vertex(x2, y1, color[1])
        self._vertex(x1, y2, color[3])
        self._vertex(x2, y2, color[2])
        self.indices.extend([base + 0, base + 2, base + 1, base + 1, base + 2, base + 3])

    # Create a smooth vertical gradient by subdividing into multiple strips.
    # gamma: low gamma below 0.5 is good
    def add_smooth_vertical_gradient(self, x1, y1, x2, y2, top_color, bottom_color, strips, gamma):
        for i in range(strips):
            t0 = i / strips
            t1 = (i + 1) / strips

            # Interpolate colors with gamma correction
            t0_color = t0 ** gamma
            t1_color = t1 ** gamma
            color_top = [top_color[j] + (bottom_color[j] - top_color[j]) * t0_color for j in range(4)]
            color_bottom = [top_color[j] + (bottom_color[j] - top_color[j]) * t1_color for j in range(4)]

            # Y coordinates use linear spacing (equal strip heights)
            y_top = _add(_mul(1.0 - t0, y1), _mul(t0, y2))
            y_bottom = _add(_mul(1.0 - t1, y1), _mul(t1, y2))

            self.add_gradient_box(
                x1,
                y_top,
                x2,
                y_bottom,
                [color_top, color_top, color_bottom, color_bottom],
            )

    def add_outline(self, x1, y1, x2, y2, border_px, color):
        # LHS
        self.add_filled_box(x1, y1, _add(x1, border_px), y2, color)
        # THS
        self.add_filled_box(x1, y1, x2, _add(y1, border_px), color)
        # RHS
        self.add_filled_box(_add(x2, -border_px), y1, x2, y2, color)
        # BHS
        self.add_filled_box(x1, _add(y2, -border_px), x2, y2, color)

    # Draw a line of a certain thickness between two points.
    # Coordinates are constants, so this does not track expressions like `w` or `h`.
    def add_line(self, from_x, from_y, to_x, to_y, thickness, color):
        dx = to_x - from_x
        dy = to_y - from_y
        length = math.sqrt(dx * dx + dy * dy)
        if length == 0.:
            return

        half = thickness / 2.
        px = -dy / length * half
        py = dx / length * half

        base = len(self.verts)
        self._vertex(from_x + px, from_y + py, color)
        self._vertex(to_x + px, to_y + py, color)
        self._vertex(from_x - px, from_y - py, color)
        self._vertex(to_x - px, to_y - py, color)
        self.indices.extend([base, base + 2, base + 1, base + 1, base + 2, base + 3])

    def add_radial_glow(self, center_x, center_y, width, height, segments, start_angle, end_angle, color):
        def ellipse_x(cos_angle):
            return _add(center_x, _mul(width, cos_angle * 0.5))

        def ellipse_y(sin_angle):
            return _add(center_y, _mul(height, sin_angle * 0.5))

        base = len(self.verts)
        self._vertex(center_x, center_y, color)

        arc_color = list(color)
        arc_color[3] = 0.
        for i in range(segments + 1):
            t = i / segments
            angle = start_angle + t * (end_angle - start_angle)
            self._vertex(ellipse_x(math.cos(angle)), ellipse_y(math.sin(angle)), arc_color)

        for i in range(segments):
            self.indices.extend([base, base + 1 + i, base + 2 + i])

    def scaled(self, scale):
        shape = VectorShape()
        shape.verts = [[_mul(scale, v[0]), _mul(scale, v[1]), list(v[2])] for v in self.verts]
        shape.indices = list(self.indices)
        return shape

    def offset(self, off_x, off_y):
        shape = VectorShape()
        shape.verts = [[_add(v[0], off_x), _add(v[1], off_y), list(v[2])] for v in self.verts]
        shape.indices = list(self.indices)
        return shape

# python -m pydrk.vector_shape
if __name__ == "__main__":
    assert _coord(6.123233995736766e-17) == f"{6.123233995736766e-17:.30f}".rstrip("0")
    assert _coord(-1.2246467991473532e-16) == f"(0 - {f'{1.2246467991473532e-16:.30f}'.rstrip('0')})"
    assert _coord(0.0) == "0.0"
    assert _coord(10) == "10.0"
    assert _coord(-4.0) == "(0 - 4.0)"

    shape = VectorShape()
    shape.add_filled_box("w/2", 0, "w", 10, [1., 0., 0., 1.])
    assert len(shape.verts) == 4 and len(shape.indices) == 6
    assert shape.verts[0][0] == "w/2" and shape.verts[2][1] == "10.0"
    assert shape.indices == [0, 2, 1, 1, 2, 3]

    shape = VectorShape()
    shape.add_smooth_vertical_gradient(0, 0, 10, 100, [1., 1., 1., 1.], [0., 0., 0., 0.], 8, 0.45)
    assert len(shape.verts) == 8 * 4 and len(shape.indices) == 8 * 6
    assert shape.verts[0][1] == "((1.0 * 0.0) + (0.0 * 100.0))"
    assert shape.verts[3][1] == "((0.875 * 0.0) + (0.125 * 100.0))"

    shape = VectorShape()
    shape.add_outline("x1", "y1", "x2", "y2", 2.0, [0., 0., 0., 1.])
    assert len(shape.verts) == 16 and len(shape.indices) == 24
    assert shape.verts[1][0] == "(x1 + 2.0)"
    assert shape.verts[8][0] == "(x2 + (0 - 2.0))"
    assert shape.verts[12][1] == "(y2 + (0 - 2.0))"

    shape = VectorShape()
    shape.add_line(0., 0., 10., 0., 4., [1., 1., 1., 1.])
    assert len(shape.verts) == 4 and len(shape.indices) == 6
    assert shape.verts[0][1] == "2.0" and shape.verts[2][1] == "(0 - 2.0)"

    shape = VectorShape()
    shape.add_radial_glow("w/2", "h/2", "w", "h", 12, 0., math.pi * 2., [1., 0., 0., 1.])
    assert len(shape.verts) == 14 and len(shape.indices) == 36
    assert shape.verts[0][0] == "w/2"
    assert shape.verts[1][0] == "(w/2 + (w * 0.5))"
    assert shape.verts[13][2][3] == 0.

    a = VectorShape()
    a.add_filled_box(0, 0, 1, 1, [1., 1., 1., 1.])
    b = VectorShape()
    b.add_filled_box(0, 0, 1, 1, [0., 0., 0., 1.])
    a.join(b)
    assert len(a.verts) == 8 and a.indices[6:] == [4, 6, 5, 5, 6, 7]

    s = a.scaled(2.5)
    assert s.verts[0][0] == "(2.5 * 0.0)"
    o = a.offset(10., 20.)
    assert o.verts[4][0] == "(0.0 + 10.0)" and o.verts[5][1] == "(0.0 + 20.0)"

    print("vector_shape self-test OK")
