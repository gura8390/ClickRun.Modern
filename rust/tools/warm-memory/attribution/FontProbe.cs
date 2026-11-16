using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;

namespace ClickRun.WarmMemoryAttribution {
    public sealed class FontTable {
        public string Tag { get; set; }
        public long Length { get; set; }
        public string Sha256 { get; set; }
        public string Error { get; set; }
        public bool? MatchesSource { get; set; }
    }
    public sealed class FontSelection {
        public string RequestedFace { get; set; }
        public int LogicalPixels { get; set; }
        public int RequestedHeight { get; set; }
        public string SelectedFace { get; set; }
        public TextMetrics Metrics { get; set; }
        public long FontDataLength { get; set; }
        public FontTable[] Tables { get; set; }
        public bool AllSourceTablesMatch { get; set; }
        public string Error { get; set; }
    }
    [StructLayout(LayoutKind.Sequential, CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
    public struct TextMetrics {
        public int Height, Ascent, Descent, InternalLeading, ExternalLeading;
        public int AveCharWidth, MaxCharWidth, Weight, Overhang, DigitizedAspectX, DigitizedAspectY;
        public char FirstChar, LastChar, DefaultChar, BreakChar;
        public byte Italic, Underlined, StruckOut, PitchAndFamily, CharSet;
    }
    public sealed class FontProbeReport {
        public int SchemaVersion { get; set; }
        public int ProcessId { get; set; }
        public string StartedUtc { get; set; }
        public string EndedUtc { get; set; }
        public int PointerBytes { get; set; }
        public int Dpi { get; set; }
        public string FontFile { get; set; }
        public long SourceFileLength { get; set; }
        public string SourceSha256 { get; set; }
        public bool PrivateRegistrationRequested { get; set; }
        public int RegisteredFaces { get; set; }
        public bool? PrivateRegistrationRemoved { get; set; }
        public FontTable[] SourceTables { get; set; }
        public FontSelection[] Selections { get; set; }
        public string Scope { get; set; }
        public bool Passed { get; set; }
        public string Failure { get; set; }
    }
    public static class FontProbe {
        const uint PrivateFont = 0x10, GdiError = 0xffffffff;
        const int TableByteLimit = 2 * 1024 * 1024;
        static readonly string[] TableTags = { "name", "head", "maxp", "OS/2", "hhea", "cmap" };
        [DllImport("gdi32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
        static extern int AddFontResourceExW(string file, uint flags, IntPtr reserved);
        [DllImport("gdi32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        static extern bool RemoveFontResourceExW(string file, uint flags, IntPtr reserved);
        [DllImport("gdi32.dll", SetLastError = true)]
        static extern IntPtr CreateCompatibleDC(IntPtr dc);
        [DllImport("gdi32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        static extern bool DeleteDC(IntPtr dc);
        [DllImport("gdi32.dll")]
        static extern IntPtr SelectObject(IntPtr dc, IntPtr value);
        [DllImport("gdi32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        static extern bool DeleteObject(IntPtr value);
        [DllImport("gdi32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
        static extern IntPtr CreateFontW(int height, int width, int escapement, int orientation,
            int weight, uint italic, uint underline, uint strikeout, uint charSet,
            uint outputPrecision, uint clipPrecision, uint quality, uint pitchAndFamily, string face);
        [DllImport("gdi32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
        static extern int GetTextFaceW(IntPtr dc, int count, StringBuilder face);
        [DllImport("gdi32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        static extern bool GetTextMetricsW(IntPtr dc, out TextMetrics metrics);
        [DllImport("gdi32.dll")]
        static extern uint GetFontData(IntPtr dc, uint table, uint offset, [Out] byte[] data, uint length);

        static string Hash(byte[] data) {
            using (var sha = SHA256.Create()) return BitConverter.ToString(sha.ComputeHash(data)).Replace("-", "");
        }
        static string HashFile(string file) {
            using (var sha = SHA256.Create())
            using (var stream = File.OpenRead(file)) return BitConverter.ToString(sha.ComputeHash(stream)).Replace("-", "");
        }
        static void ReadExactly(Stream stream, byte[] data) {
            int position = 0;
            while (position < data.Length) {
                int read = stream.Read(data, position, data.Length - position);
                if (read == 0) throw new EndOfStreamException("字体表未完整读取。");
                position += read;
            }
        }
        static uint Big32(byte[] bytes, int position) {
            return ((uint)bytes[position] << 24) | ((uint)bytes[position + 1] << 16)
                | ((uint)bytes[position + 2] << 8) | bytes[position + 3];
        }
        // GDI uses a DWORD whose in-memory byte order is the four ASCII tag bytes.
        static uint GdiTag(string tag) {
            return (uint)tag[0] | ((uint)tag[1] << 8) | ((uint)tag[2] << 16) | ((uint)tag[3] << 24);
        }
        static FontTable[] ReadSourceTables(string file) {
            using (var stream = File.OpenRead(file)) {
                byte[] header = new byte[12];
                ReadExactly(stream, header);
                if (Encoding.ASCII.GetString(header, 0, 4) == "ttcf")
                    throw new NotSupportedException("本探针的源文件比对限定为单字体TTF/OTF，未将TTC集合当作单字体处理。");
                int count = (header[4] << 8) | header[5];
                if (count <= 0 || count > 4096) throw new InvalidDataException("字体表目录数量无效。");
                var directory = new Dictionary<string, Tuple<uint, uint>>();
                for (int i = 0; i < count; i++) {
                    byte[] row = new byte[16];
                    ReadExactly(stream, row);
                    directory.Add(Encoding.ASCII.GetString(row, 0, 4), Tuple.Create(Big32(row, 8), Big32(row, 12)));
                }
                var result = new List<FontTable>();
                foreach (string tag in TableTags) {
                    var item = new FontTable { Tag = tag };
                    Tuple<uint, uint> entry;
                    if (!directory.TryGetValue(tag, out entry)) item.Error = "源文件中没有此字体表。";
                    else {
                        item.Length = entry.Item2;
                        if (entry.Item2 > TableByteLimit || (long)entry.Item1 + entry.Item2 > stream.Length)
                            throw new InvalidDataException("字体表越界或超过探针2MiB单表上限：" + tag);
                        stream.Position = entry.Item1;
                        byte[] data = new byte[(int)entry.Item2];
                        ReadExactly(stream, data);
                        item.Sha256 = Hash(data);
                    }
                    result.Add(item);
                }
                return result.ToArray();
            }
        }
        static FontTable ReadSelectedTable(IntPtr dc, FontTable source) {
            var result = new FontTable { Tag = source.Tag };
            uint size = GetFontData(dc, GdiTag(source.Tag), 0, null, 0);
            if (size == GdiError) { result.Error = "所选字体没有可读取的此TrueType表。"; return result; }
            result.Length = size;
            if (size > TableByteLimit) { result.Error = "字体表超过探针2MiB单表上限。"; return result; }
            byte[] bytes = new byte[(int)size];
            uint read = GetFontData(dc, GdiTag(source.Tag), 0, bytes, size);
            if (read != size) { result.Error = "GetFontData未完整读取字体表。"; return result; }
            result.Sha256 = Hash(bytes);
            result.MatchesSource = source.Sha256 != null && source.Length == size && source.Sha256 == result.Sha256;
            return result;
        }
        static FontSelection Select(string face, int logicalPixels, int dpi, FontTable[] source) {
            var result = new FontSelection { RequestedFace = face, LogicalPixels = logicalPixels,
                RequestedHeight = -logicalPixels * dpi / 96 };
            IntPtr dc = CreateCompatibleDC(IntPtr.Zero), font = IntPtr.Zero, previous = IntPtr.Zero;
            if (dc == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "无法创建探针内存DC。");
            try {
                // Match src/ui.rs make_fonts: width0, weight400, DEFAULT_CHARSET1, CLEARTYPE_QUALITY5.
                font = CreateFontW(result.RequestedHeight, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, face);
                if (font == IntPtr.Zero) throw new InvalidOperationException("无法创建探针逻辑字体。");
                previous = SelectObject(dc, font);
                if (previous == IntPtr.Zero || previous == new IntPtr(-1)) throw new InvalidOperationException("无法选择探针逻辑字体。");
                var name = new StringBuilder(256);
                if (GetTextFaceW(dc, name.Capacity, name) == 0) throw new InvalidOperationException("无法读取所选字体名称。");
                result.SelectedFace = name.ToString();
                TextMetrics metrics;
                if (!GetTextMetricsW(dc, out metrics)) throw new InvalidOperationException("无法读取所选字体度量。");
                result.Metrics = metrics;
                uint length = GetFontData(dc, 0, 0, null, 0);
                result.FontDataLength = length == GdiError ? -1 : length;
                result.Tables = source.Select(table => ReadSelectedTable(dc, table)).ToArray();
                result.AllSourceTablesMatch = result.Tables.Length > 0 && result.Tables.All(t => t.MatchesSource == true);
            } catch (Exception exception) { result.Error = exception.Message; }
            finally {
                if (previous != IntPtr.Zero && previous != new IntPtr(-1)) SelectObject(dc, previous);
                if (font != IntPtr.Zero) DeleteObject(font);
                DeleteDC(dc);
            }
            return result;
        }
        public static FontProbeReport Run(string sourceFile, bool registerPrivate, int dpi, string requestedFace) {
            if (IntPtr.Size != 8) throw new NotSupportedException("请使用64位独立PowerShell进程。");
            if (dpi < 48 || dpi > 768) throw new ArgumentOutOfRangeException("dpi", "DPI必须在48至768之间。");
            string fullPath = Path.GetFullPath(sourceFile);
            var result = new FontProbeReport { SchemaVersion = 1, ProcessId = Process.GetCurrentProcess().Id,
                StartedUtc = DateTimeOffset.UtcNow.ToString("o"), PointerBytes = IntPtr.Size, Dpi = dpi,
                FontFile = fullPath, PrivateRegistrationRequested = registerPrivate,
                Scope = "仅本独立探针进程的内存DC和FR_PRIVATE临时字体；无窗口、无输入、无系统字体安装、无目标进程DC或HFONT操作。该探针会预热系统字体读取缓存，不作为应用内存样本。" };
            try {
                result.SourceFileLength = new FileInfo(fullPath).Length;
                result.SourceSha256 = HashFile(fullPath);
                result.SourceTables = ReadSourceTables(fullPath);
                if (registerPrivate) {
                    result.RegisteredFaces = AddFontResourceExW(fullPath, PrivateFont, IntPtr.Zero);
                    if (result.RegisteredFaces <= 0) throw new InvalidOperationException("FR_PRIVATE字体注册失败。");
                }
                result.Selections = new[] {14, 28, 12, 24}.Select(size => Select(requestedFace, size, dpi, result.SourceTables)).ToArray();
                result.Passed = result.Selections.All(s => s.Error == null)
                    && (!registerPrivate || result.Selections.All(s => s.AllSourceTablesMatch));
                if (!result.Passed) result.Failure = "所选字体读取失败，或私有注册分支的字体表未完整匹配源TTF。";
            } catch (Exception exception) { result.Failure = exception.Message; }
            finally {
                if (result.RegisteredFaces > 0) {
                    result.PrivateRegistrationRemoved = RemoveFontResourceExW(fullPath, PrivateFont, IntPtr.Zero);
                    if (result.PrivateRegistrationRemoved != true) {
                        result.Passed = false;
                        result.Failure = "探针私有字体注册清理失败。";
                    }
                }
                result.EndedUtc = DateTimeOffset.UtcNow.ToString("o");
            }
            return result;
        }
    }
}
