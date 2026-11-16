using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Cryptography;

namespace ClickRun.WarmMemoryAttribution {
    public sealed class IconRenderComparison {
        public int Frame { get; set; }
        public int Width { get; set; }
        public int Height { get; set; }
        public string Background { get; set; }
        public string SourceRgbSha256 { get; set; }
        public string DiagnosticRgbSha256 { get; set; }
        public int DifferentPixels { get; set; }
        public int MaxChannelDifference { get; set; }
        public bool Passed { get; set; }
    }
    public sealed class IconRenderReport {
        public int SchemaVersion { get; set; }
        public string Source { get; set; }
        public string Diagnostic { get; set; }
        public string SourceSha256 { get; set; }
        public string DiagnosticSha256 { get; set; }
        public string CapturedUtc { get; set; }
        public bool Passed { get; set; }
        public string Failure { get; set; }
        public IconRenderComparison[] Comparisons { get; set; }
        public string Method { get; set; }
    }
    public static class IconRenderProbe {
        [StructLayout(LayoutKind.Sequential)]
        struct BitmapInfo {
            public uint Size;
            public int Width, Height;
            public ushort Planes, BitCount;
            public uint Compression, SizeImage;
            public int XPelsPerMeter, YPelsPerMeter;
            public uint ClrUsed, ClrImportant;
        }
        sealed class Frame { public int Width, Height; public byte[] Bytes; }
        [DllImport("user32.dll", SetLastError = true)]
        static extern IntPtr CreateIconFromResourceEx(byte[] bytes, uint size, [MarshalAs(UnmanagedType.Bool)] bool icon, uint version, int width, int height, uint flags);
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        static extern bool DrawIconEx(IntPtr dc, int x, int y, IntPtr icon, int width, int height, uint step, IntPtr brush, uint flags);
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        static extern bool DestroyIcon(IntPtr icon);
        [DllImport("gdi32.dll", SetLastError = true)]
        static extern IntPtr CreateCompatibleDC(IntPtr dc);
        [DllImport("gdi32.dll", SetLastError = true)]
        static extern IntPtr CreateDIBSection(IntPtr dc, ref BitmapInfo info, uint usage, out IntPtr bits, IntPtr section, uint offset);
        [DllImport("gdi32.dll")]
        static extern IntPtr SelectObject(IntPtr dc, IntPtr value);
        [DllImport("gdi32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        static extern bool DeleteObject(IntPtr value);
        [DllImport("gdi32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        static extern bool DeleteDC(IntPtr dc);
        [DllImport("gdi32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        static extern bool GdiFlush();

        static string Hash(byte[] value) {
            using (var sha = SHA256.Create()) return BitConverter.ToString(sha.ComputeHash(value)).Replace("-", "");
        }
        static Frame[] Frames(byte[] file) {
            if (file.Length < 6 || BitConverter.ToUInt16(file, 0) != 0 || BitConverter.ToUInt16(file, 2) != 1)
                throw new InvalidDataException("不是有效ICO文件。");
            int count = BitConverter.ToUInt16(file, 4);
            if (count <= 0 || count > 256 || file.Length < 6 + count * 16) throw new InvalidDataException("ICO帧目录无效。");
            var result = new List<Frame>();
            for (int i = 0; i < count; i++) {
                int row = 6 + i * 16;
                uint length = BitConverter.ToUInt32(file, row + 8), offset = BitConverter.ToUInt32(file, row + 12);
                if (length > 16 * 1024 * 1024 || offset < 6 + count * 16 || (ulong)offset + length > (ulong)file.Length)
                    throw new InvalidDataException("ICO帧数据越界。");
                byte[] image = new byte[(int)length];
                Array.Copy(file, (long)offset, image, 0L, (long)length);
                result.Add(new Frame { Width = file[row] == 0 ? 256 : file[row], Height = file[row + 1] == 0 ? 256 : file[row + 1], Bytes = image });
            }
            return result.ToArray();
        }
        static byte[] Render(Frame frame, byte background) {
            IntPtr icon = IntPtr.Zero, dc = IntPtr.Zero, bitmap = IntPtr.Zero, old = IntPtr.Zero;
            try {
                icon = CreateIconFromResourceEx(frame.Bytes, (uint)frame.Bytes.Length, true, 0x00030000, frame.Width, frame.Height, 0);
                if (icon == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "系统无法解码图标帧。");
                dc = CreateCompatibleDC(IntPtr.Zero);
                if (dc == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "无法创建离屏内存DC。");
                var info = new BitmapInfo { Size = 40, Width = frame.Width, Height = -frame.Height, Planes = 1, BitCount = 32 };
                IntPtr bits;
                bitmap = CreateDIBSection(dc, ref info, 0, out bits, IntPtr.Zero, 0);
                if (bitmap == IntPtr.Zero || bits == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "无法创建离屏DIB。");
                old = SelectObject(dc, bitmap);
                if (old == IntPtr.Zero || old == new IntPtr(-1)) throw new InvalidOperationException("无法选择离屏DIB。");
                byte[] bgra = new byte[frame.Width * frame.Height * 4];
                for (int i = 0; i < bgra.Length; i += 4) { bgra[i] = bgra[i + 1] = bgra[i + 2] = background; bgra[i + 3] = 255; }
                Marshal.Copy(bgra, 0, bits, bgra.Length);
                if (!DrawIconEx(dc, 0, 0, icon, frame.Width, frame.Height, 0, IntPtr.Zero, 3))
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "DrawIconEx离屏渲染失败。");
                if (!GdiFlush()) throw new InvalidOperationException("GDI批处理提交失败。");
                Marshal.Copy(bits, bgra, 0, bgra.Length);
                // Compare visible RGB. Destination DIB alpha is not a visible colour;
                // exact source RGBA equality is separately checked by icon_to_dib.py.
                byte[] rgb = new byte[frame.Width * frame.Height * 3];
                for (int i = 0, j = 0; i < bgra.Length; i += 4) { rgb[j++] = bgra[i + 2]; rgb[j++] = bgra[i + 1]; rgb[j++] = bgra[i]; }
                return rgb;
            } finally {
                if (old != IntPtr.Zero && old != new IntPtr(-1)) SelectObject(dc, old);
                if (bitmap != IntPtr.Zero) DeleteObject(bitmap);
                if (dc != IntPtr.Zero) DeleteDC(dc);
                if (icon != IntPtr.Zero) DestroyIcon(icon);
            }
        }
        public static IconRenderReport Compare(string source, string diagnostic) {
            var report = new IconRenderReport { SchemaVersion = 1, Source = Path.GetFullPath(source), Diagnostic = Path.GetFullPath(diagnostic),
                CapturedUtc = DateTimeOffset.UtcNow.ToString("o"), Method = "每帧CreateIconFromResourceEx→DrawIconEx(DI_NORMAL)→32bpp离屏DIB；黑白两背景逐RGB字节比较。无窗口、无桌面DC、无输入、无目标进程操作。" };
            var rows = new List<IconRenderComparison>();
            try {
                byte[] before = File.ReadAllBytes(report.Source), after = File.ReadAllBytes(report.Diagnostic);
                report.SourceSha256 = Hash(before); report.DiagnosticSha256 = Hash(after);
                Frame[] a = Frames(before), b = Frames(after);
                if (a.Length != b.Length) throw new InvalidDataException("转换前后帧数不同。");
                for (int index = 0; index < a.Length; index++) {
                    if (a[index].Width != b[index].Width || a[index].Height != b[index].Height) throw new InvalidDataException("转换前后帧顺序或尺寸不同。");
                    foreach (byte bg in new byte[] { 0, 255 }) {
                        byte[] first = Render(a[index], bg), second = Render(b[index], bg);
                        int pixels = 0, maximum = 0;
                        for (int i = 0; i < first.Length; i += 3) {
                            bool different = false;
                            for (int j = 0; j < 3; j++) { int delta = Math.Abs(first[i + j] - second[i + j]); maximum = Math.Max(maximum, delta); different |= delta != 0; }
                            if (different) pixels++;
                        }
                        rows.Add(new IconRenderComparison { Frame = index, Width = a[index].Width, Height = a[index].Height,
                            Background = bg == 0 ? "黑色" : "白色", SourceRgbSha256 = Hash(first), DiagnosticRgbSha256 = Hash(second),
                            DifferentPixels = pixels, MaxChannelDifference = maximum, Passed = pixels == 0 });
                    }
                }
                report.Passed = rows.Count > 0 && rows.All(row => row.Passed);
                if (!report.Passed) report.Failure = "系统离屏渲染结果存在RGB差异。";
            } catch (Exception error) { report.Failure = error.Message; }
            report.Comparisons = rows.ToArray();
            return report;
        }
    }
}
