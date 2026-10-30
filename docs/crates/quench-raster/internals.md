---
title: Internals
parent: quench-raster
nav_order: 3
---

## Pixel format

`bytes()` holds, per pixel, `[blue, green, red, alpha]`, each channel premultiplied by alpha. This is the layout of Wayland `ARGB8888` and of a little-endian `u32` `0xAARRGGBB`, so a frame goes to a compositor unchanged. Rows are `width * 4` bytes, no padding.

## Arithmetic

`mul255(a, b) = ((a*b + 128) + ((a*b + 128) >> 8)) >> 8`, that is `a*b/255` rounded (the function is crate-private). Blending a colour with coverage `c` (0..=255): `alpha = mul255(color.a, c)`; each channel `= mul255(src, alpha) + mul255(dst, 255 - alpha)`, capped at 255; alpha `= alpha + mul255(dst_a, 255 - alpha)`.

## Rectangles with rounded corners

Anti-aliasing for `fill_rounded_rect`, `fill_rrect`, `stroke_*` and `shadow_rrect` comes from a signed distance `sd` from the pixel centre `(x+0.5, y+0.5)` to the box (negative inside). The radius of the quadrant the point is in is clamped to `0..=min(w,h)/2`. Coverage is `clamp(0.5 - sd, 0, 1)`. A stroke is `coverage(sd) * (1 - coverage(sd + width))`. A blurred shadow edge is `0.5 * (1 - erf(sd / (sigma * sqrt 2)))` with `sigma = blur / 2`; `erf` is Abramowitz and Stegun 7.1.26 (error below 1.5e-7). The area visited is the box (grown by `3*sigma` for outer shadows) plus one pixel on each side, intersected with the clip.

## Paths

`fill_path` uses signed-area accumulation: each edge adds the area it sweeps to the cells it crosses in one pass, and a running sum along each row gives coverage (`abs(sum)` capped at 1, times 255, rounded). Cost is the edges plus the covered area. Edges are split at the left and right sides of the visited window so the part inside keeps its slope while the winding contribution of the part outside is kept as a vertical edge on the side. Only the intersection of the path bounds with the clip is rasterised (scratch memory `(w+2) * h` floats).

Curves flatten at insertion: segments `n = clamp(ceil(sqrt(factor * d / (8 * 0.02))), 2, 128)` where `d` is the second difference (quadratic: `|p0 - 2c + p1|`, factor 2; cubic: the larger of the two second differences of the control polygon, factor 6).

## Performance notes

Opaque `fill_rect` writes slices per row. `fill_rrect` on whole-pixel boxes fills the straight middle with `fill_rect` (a full-screen background costs a memory fill). Everything else visits pixels one by one with `f32` maths; `draw_layer` and `draw_layer_transformed` are per-pixel loops over the layer or the destination box.
