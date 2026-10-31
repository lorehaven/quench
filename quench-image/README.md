# quench-image
Image decoding with no dependencies: PNG, JPEG, GIF and WebP into RGBA, and a PNG encoder.

## What it is
`quench-image` decodes raster images to straight-alpha RGBA and encodes PNG. Its only dependency is `quench-archive` (inflate, deflate, CRC-32, Adler-32). It replaces the `image`, `png`, `jpeg-decoder`, `gif` and `webp` crates for the native renderer's `<img>` (`quench-ui-native`); vector drawings are `quench-svg`.

The format is chosen from the first bytes. Images larger than 16 384 pixels a side or 64 Mi pixels in total give `Error::TooLarge`, and damaged input is an `Error`, never a panic.

## Use cases
- Load a PNG, JPEG, GIF or WebP file into a pixel buffer.
- Convert any readable image to PNG.
- Decode untrusted uploads with size limits checked before allocation.
- Save the output of `quench_svg::render` as a PNG.

## Example
```rust
let bytes = std::fs::read("photo.jpg")?;
let image = quench_image::decode(&bytes)?;       // Image { width, height, rgba }
assert_eq!(image.rgba.len(), image.width as usize * image.height as usize * 4);
let png = quench_image::encode_png(image.width, image.height, &image.rgba);
std::fs::write("photo.png", png)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-image/
