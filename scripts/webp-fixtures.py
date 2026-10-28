#!/usr/bin/env python3
"""Makes the lossy WebP fixtures of quench-image/tests/data with the options Pillow does not offer, by calling
libwebp's encoder through ctypes, and writes beside each the pixels libwebp makes of it (`name.webp.rgba`).

    python3 scripts/webp-fixtures.py            # needs libwebp and Pillow
"""
import ctypes, math, os, sys
from PIL import Image

lib = ctypes.CDLL("libwebp.so.7")
DATA = os.path.join(os.path.dirname(__file__), "..", "quench-image", "tests", "data")

class Config(ctypes.Structure):
    _fields_ = [(n, t) for n, t in [
        ("lossless", ctypes.c_int), ("quality", ctypes.c_float), ("method", ctypes.c_int), ("image_hint", ctypes.c_int),
        ("target_size", ctypes.c_int), ("target_PSNR", ctypes.c_float), ("segments", ctypes.c_int),
        ("sns_strength", ctypes.c_int), ("filter_strength", ctypes.c_int), ("filter_sharpness", ctypes.c_int),
        ("filter_type", ctypes.c_int), ("autofilter", ctypes.c_int), ("alpha_compression", ctypes.c_int),
        ("alpha_filtering", ctypes.c_int), ("alpha_quality", ctypes.c_int), ("pass_", ctypes.c_int),
        ("show_compressed", ctypes.c_int), ("preprocessing", ctypes.c_int), ("partitions", ctypes.c_int),
        ("partition_limit", ctypes.c_int), ("emulate_jpeg_size", ctypes.c_int), ("thread_level", ctypes.c_int),
        ("low_memory", ctypes.c_int), ("near_lossless", ctypes.c_int), ("exact", ctypes.c_int),
        ("use_delta_palette", ctypes.c_int), ("use_sharp_yuv", ctypes.c_int), ("qmin", ctypes.c_int),
        ("qmax", ctypes.c_int)]]

WRITER = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.POINTER(ctypes.c_uint8), ctypes.c_size_t, ctypes.c_void_p)

class Picture(ctypes.Structure):
    _fields_ = [("use_argb", ctypes.c_int), ("colorspace", ctypes.c_int), ("width", ctypes.c_int), ("height", ctypes.c_int),
        ("y", ctypes.c_void_p), ("u", ctypes.c_void_p), ("v", ctypes.c_void_p), ("y_stride", ctypes.c_int),
        ("uv_stride", ctypes.c_int), ("a", ctypes.c_void_p), ("a_stride", ctypes.c_int), ("pad1", ctypes.c_uint32 * 2),
        ("argb", ctypes.c_void_p), ("argb_stride", ctypes.c_int), ("pad2", ctypes.c_uint32 * 3), ("writer", WRITER),
        ("custom_ptr", ctypes.c_void_p), ("extra_info_type", ctypes.c_int), ("extra_info", ctypes.c_void_p),
        ("stats", ctypes.c_void_p), ("error_code", ctypes.c_int), ("progress_hook", ctypes.c_void_p),
        ("user_data", ctypes.c_void_p), ("pad3", ctypes.c_uint32 * 3), ("pad4", ctypes.c_void_p), ("pad5", ctypes.c_void_p),
        ("pad6", ctypes.c_uint32 * 8), ("memory_", ctypes.c_void_p), ("memory_argb_", ctypes.c_void_p),
        ("pad7", ctypes.c_void_p * 2)]

def encode(image, **options):
    config, picture = Config(), Picture()
    assert lib.WebPConfigInitInternal(ctypes.byref(config), 0, ctypes.c_float(75.0), 0x0210)
    assert lib.WebPPictureInitInternal(ctypes.byref(picture), 0x0210)
    for key, value in options.items():
        setattr(config, key, value)
    picture.use_argb = 0
    picture.width, picture.height = image.size
    rgba = image.convert("RGBA").tobytes()
    assert lib.WebPPictureImportRGBA(ctypes.byref(picture), rgba, image.width * 4)
    chunks = []
    def write(data, size, _):
        chunks.append(ctypes.string_at(data, size))
        return 1
    picture.writer = WRITER(write)
    assert lib.WebPEncode(ctypes.byref(config), ctypes.byref(picture)), picture.error_code
    lib.WebPPictureFree(ctypes.byref(picture))
    return b"".join(chunks)

def scene(w, h, alpha=False):
    im = Image.new("RGBA", (w, h))
    px = im.load()
    for y in range(h):
        for x in range(w):
            r = int(127 + 127 * math.sin(x / 7.0 + y / 11.0))
            g = int(255 * x / max(1, w - 1))
            b = int(255 * y / max(1, h - 1)) ^ (40 if (x // 9 + y // 7) % 2 else 0)
            a = 255
            if alpha:
                a = 255 if x < w // 3 else int(255 * (1 + math.sin(x / 5.0 - y / 9.0)) / 2)
            px[x, y] = (r, g, b, a)
    return im

def save(name, image, **options):
    path = os.path.join(DATA, name)
    open(path, "wb").write(encode(image, **options))
    open(path + ".rgba", "wb").write(Image.open(path).convert("RGBA").tobytes())

save("lossy_simple_filter.webp", scene(70, 50), filter_type=0, filter_strength=70, quality=60)
save("lossy_sharp_filter.webp", scene(70, 50), filter_type=1, filter_strength=100, filter_sharpness=6, quality=40)
save("lossy_partitions.webp", scene(48, 150), partitions=3, quality=70)
save("lossy_one_segment.webp", scene(40, 40), segments=1, quality=85, autofilter=1)
save("lossy_tiny.webp", scene(1, 1), quality=50)
save("lossy_odd.webp", scene(17, 31), quality=65, method=6)
save("lossy_no_filter.webp", scene(33, 33), filter_strength=0, quality=90)
for n, (compression, filtering, quality) in enumerate([(0, 0, 100), (1, 1, 100), (1, 2, 100), (1, 1, 30), (0, 1, 100)]):
    save(f"lossy_alpha{n}.webp", scene(45, 30, alpha=True), alpha_compression=compression, alpha_filtering=filtering, alpha_quality=quality, quality=70)
print("done")
