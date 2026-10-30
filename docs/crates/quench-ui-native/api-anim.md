---
title: Animation API
parent: quench-ui-native
nav_order: 3
---

Source: `src/anim.rs`. The module has no public items of its own; it is driven by the cascade, [`tick`](api-timers.md#tick), [`next_wake`](api-timers.md#next_wake) and [`advance_animations`](api-timers.md#advance_animations). Parsing of `transition`, `animation` and `@keyframes` is [quench-css](../quench-css/index.md)'s (`ComputedStyle::transitions`, `ComputedStyle::animations`, `Cascade::keyframes`, `Cascade::style_with`).

State (`Anim`, held in the DOM): the last style of every element (what the cascade said, and what was shown), running transitions by `(element, property)`, the start instant of every keyframe animation by `(element, name)`, a clock skew, and the reduced-motion flag. At the start of every layout `Anim::begin` fixes "now" (`Instant::now() + skew`) and clears the `animating` flag; during style computation `Anim::apply` is called for every element: transitions first, then keyframe animations on top.

## Transitions

Supported properties (`Prop`) and the CSS names matched in `transition-property`:

| Property group | Name matched | Values moved |
|---|---|---|
| Opacity | `opacity` | `opacity` |
| Color | `color` | text colour |
| Background | `background-color` (also `background`) | resolved background colour |
| Border | `border-color` | all four border colours together |

`all` matches every group. When several entries cover a property, the last one wins. Changes of any other property (sizes, positions, transforms) are not transitioned.

Rules:

* The first style an element gets is not transitioned (no previous style).
* A transition starts when the cascade's target value for a group differs from the previous target (floating comparison for opacity with `f32::EPSILON`, equality for colours). It starts from what is *shown* at that moment (so a change half way continues from the displayed value) and replaces a running transition of the same group.
* No transition starts when `duration + delay <= 0` or when reduced motion is on (`MediaEnv::reduced_motion`): the change is immediate.
* Progress: `elapsed = now - start - delay`; `t = clamp(elapsed / duration)`, eased by the timing function; finished when `elapsed >= duration`.
* Timing functions: `Linear`, `Ease` (0.25, 0.1, 0.25, 1), `EaseIn` (0.42, 0, 1, 1), `EaseOut` (0, 0, 0.58, 1), `EaseInOut` (0.42, 0, 0.58, 1), evaluated as cubic Béziers by 24 bisection steps. `cubic-bezier()` and `steps()` are read by quench-css as `ease` and `linear`.
* Colour mixing is alpha-weighted (a clear colour does not darken the other); channels are rounded.
* `TransitionEnd` events are *not* delivered.

## Keyframe animations

`animation: name 1s linear infinite` and its longhands (`-name -duration -delay -timing-function -iteration-count -direction -fill-mode`) are parsed into `ComputedStyle::animations`; `@keyframes name` rules are looked up with `cascade.keyframes(name)`. A name without keyframes is ignored. Several animations on one element are played in order, later ones overriding the same group.

Groups interpolated (`Group`) and the declaration names that put a group in play in a keyframe:

| Group | Declarations that set it | Interpolated |
|---|---|---|
| Opacity | `opacity` | number |
| Color | `color` | colour |
| Background | `background`, `background-color` | colour |
| Border | `border`, and any `border*color` (e.g. `border-top-color`) | the four border colours |
| Transform | `transform`, `rotate` | translate (px with px, % with %; mixed units switch at 0.5), scale (x, y), rotate |

Algorithm per animation per group:

1. Each keyframe's declarations are turned into a style (`cascade.style_with(base, parent, root_font, declarations)`). Only the keyframes that set the group take part, sorted by offset.
2. If the first offset is above 0, the element's own value (`base`) is inserted at 0; if the last is below 1, at 1.
3. Progress `p` (0 to 1) comes from the timing: `elapsed = now - started - delay`. The animation start is recorded the first time the element is styled with that name and forgotten when the name is no longer in `animation-name` (so it starts again when re-added).
4. `p` is placed between the two frames around it and the timing function (animation-level, applied between each pair of frames) eases the local fraction; the group's values are blended into the style.

Timing details (`animation_progress`):

* Before the delay (`elapsed < 0`): nothing is shown unless `fill-mode` is `backwards` or `both`, in which case the start position is shown (running).
* After the end (`duration <= 0` or `elapsed >= duration * iterations`): nothing unless `forwards` or `both`; then the end position is held (a half iteration ends half way; whole counts end at the end of the last iteration, going backwards for reversed iterations).
* In between: iteration = floor(elapsed / duration), fraction = fractional part. Direction: `Normal` forward; `Reverse` backward; `Alternate` forward on even iterations; `AlternateReverse` forward on odd ones.
* `infinite` is an iteration count of infinity (quench-css).
* With reduced motion on, or no animations, nothing is played and the plain style shows.
* Sizes, layout properties and anything not in the table above are not animated.

## Frames

`Anim::active()` is true while a transition is running or a keyframe animation is running (`animating` is set when an animation was in its running part during the last style pass). While active, `layout_for_frame` recomputes the layout on every `render`, `tick` reports `true` and `next_wake` returns at most 16 ms (60 fps frames).
