"""Losslessly re-encode existing PNG ICO frames as 32bpp DIB; no artwork changes."""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import struct
import sys
from pathlib import Path

from PIL import Image, __version__ as pillow_version


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest().upper()


def frames(data: bytes) -> list[tuple[int, int, bytes]]:
    reserved, kind, count = struct.unpack_from("<HHH", data)
    if reserved != 0 or kind != 1 or not 1 <= count <= 256:
        raise ValueError("不是有效的图标目录。")
    result = []
    for index in range(count):
        w, h, _, _, _, _, length, offset = struct.unpack_from("<BBBBHHII", data, 6 + index * 16)
        if offset < 6 + count * 16 or offset + length > len(data):
            raise ValueError("图标帧越界。")
        result.append((w or 256, h or 256, data[offset : offset + length]))
    if len({(w, h) for w, h, _ in result}) != count:
        raise ValueError("本工具限定每种尺寸唯一，避免验证时选错帧。")
    return result


def ico(items: list[tuple[int, int, bytes]]) -> bytes:
    directory = bytearray(struct.pack("<HHH", 0, 1, len(items)))
    payload = bytearray()
    offset = 6 + 16 * len(items)
    for w, h, data in items:
        directory.extend(struct.pack("<BBBBHHII", w % 256, h % 256, 0, 0, 1, 32, len(data), offset))
        payload.extend(data)
        offset += len(data)
    return bytes(directory + payload)


def dib(w: int, h: int, rgba: bytes) -> bytes:
    # BI_RGB image height includes both XOR colour rows and monochrome AND mask.
    # A zero biSizeImage is permitted for BI_RGB. Colour pixels retain straight alpha.
    header = struct.pack("<IiiHHIIiiII", 40, w, h * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    colour = bytearray()
    mask_stride = ((w + 31) // 32) * 4
    mask = bytearray(mask_stride * h)
    for out_y, y in enumerate(range(h - 1, -1, -1)):
        for x in range(w):
            r, g, b, a = rgba[(y * w + x) * 4 : (y * w + x + 1) * 4]
            colour.extend((b, g, r, a))
            if a == 0:
                mask[out_y * mask_stride + x // 8] |= 0x80 >> (x % 8)
    return header + colour + mask


def convert(source: Path) -> tuple[bytes, dict]:
    original = source.read_bytes()
    converted = []
    rows = []
    for index, (w, h, png) in enumerate(frames(original)):
        if not png.startswith(b"\x89PNG\r\n\x1a\n"):
            raise ValueError("源图标必须全部为PNG帧，避免静默修改未覆盖格式。")
        with Image.open(io.BytesIO(png)) as image:
            if image.size != (w, h):
                raise ValueError("图标目录与PNG尺寸不一致。")
            pixels = image.convert("RGBA").tobytes()
        encoded = dib(w, h, pixels)
        # Decode the produced DIB ICO using Pillow's independent ICO/BMP reader.
        single_ico = ico([(w, h, encoded)])
        with Image.open(io.BytesIO(single_ico)) as check:
            decoded = check.convert("RGBA").tobytes()
        if pixels != decoded:
            raise ValueError(f"第{index}帧RGBA回读不一致。")
        alpha = pixels[3::4]
        rows.append({
            "Index": index, "Width": w, "Height": h,
            "SourceEncoding": "PNG", "OutputEncoding": "BITMAPINFOHEADER/BI_RGB/32bpp DIB",
            "SourceFrameBytes": len(png), "OutputFrameBytes": len(encoded),
            "SourceRgbaSha256": sha(pixels), "OutputRgbaSha256": sha(decoded),
            "AlphaSha256": sha(alpha), "TransparentPixels": alpha.count(0),
            "PartialAlphaPixels": sum(0 < value < 255 for value in alpha),
            "OpaquePixels": alpha.count(255), "RgbaExactlyEqual": True,
            "AndMask": "1仅用于alpha=0；底向上；每行DWORD对齐。",
        })
        converted.append((w, h, encoded))
    output = ico(converted)
    return output, {
        "SchemaVersion": 1, "Source": str(source.resolve()),
        "SourceSha256": sha(original), "SourceBytes": len(original),
        "OutputSha256": sha(output), "OutputBytes": len(output),
        "FrameCount": len(rows), "Frames": rows, "Passed": True,
        "Decoder": f"Pillow {pillow_version}; PNG decode and independent ICO/DIB decode",
        "Scope": "仅重新编码原有像素，不缩放、不调色、不预乘、不生成新图；没有调用Windows图形/字体/UI接口。",
        "SystemRender": "DrawIconEx白底黑底等效校验由独立系统探针另行记录。",
    }


def main() -> None:
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if args.destination.resolve() == args.source.resolve():
        raise ValueError("不得覆盖原始图标。")
    if args.destination.exists() or args.report.exists():
        raise FileExistsError("输出图标或报告已存在，未覆盖。")
    output, report = convert(args.source)
    report["Output"] = str(args.destination.resolve())
    with args.destination.open("xb") as stream:
        stream.write(output)
    with args.report.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(report, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    print(json.dumps(report, ensure_ascii=False))


if __name__ == "__main__":
    main()
