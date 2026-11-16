using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;

namespace ClickRun.ReadOnlyMemoryProfile {
    public sealed class ResidentGroup {
        public string Kind { get; set; }
        public string File { get; set; }
        public string AllocationBase { get; set; }
        public long Pages { get; set; }
        public long PrivatePages { get; set; }
        public double ResidentMiB { get { return Pages * 4096.0 / 1048576.0; } }
        public double PrivateResidentMiB { get { return PrivatePages * 4096.0 / 1048576.0; } }
    }
    public sealed class ResidentSnapshot {
        public int ProcessId { get; set; }
        public string ProcessPath { get; set; }
        public string CapturedUtc { get; set; }
        public long ResidentPages { get; set; }
        public long PrivateResidentPages { get; set; }
        public double ResidentMiB { get { return ResidentPages * 4096.0 / 1048576.0; } }
        public double PrivateResidentMiB { get { return PrivateResidentPages * 4096.0 / 1048576.0; } }
        public double ShareableResidentMiB { get { return (ResidentPages - PrivateResidentPages) * 4096.0 / 1048576.0; } }
        public int QueryWorkingSetAttempts { get; set; }
        public int VirtualQueryFailures { get; set; }
        public ResidentGroup[] Categories { get; set; }
        public ResidentGroup[] Files { get; set; }
        public ResidentGroup[] PrivateAllocations { get; set; }
    }
    public static class Reader {
        [StructLayout(LayoutKind.Sequential)]
        struct MemoryInfo {
            public IntPtr BaseAddress;
            public IntPtr AllocationBase;
            public uint AllocationProtect;
            public UIntPtr RegionSize;
            public uint State;
            public uint Protect;
            public uint Type;
        }
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern IntPtr OpenProcess(uint access, bool inherit, int processId);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool CloseHandle(IntPtr handle);
        [DllImport("psapi.dll", SetLastError = true)]
        static extern bool QueryWorkingSet(IntPtr process, IntPtr buffer, int size);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern UIntPtr VirtualQueryEx(IntPtr process, IntPtr address, out MemoryInfo information, UIntPtr length);
        [DllImport("psapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        static extern uint GetMappedFileNameW(IntPtr process, IntPtr address, StringBuilder fileName, int capacity);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        static extern bool QueryFullProcessImageNameW(IntPtr process, uint flags, StringBuilder fileName, ref int size);

        static string Kind(uint type) {
            if (type == 0x20000) return "MEM_PRIVATE";
            if (type == 0x1000000) return "MEM_IMAGE";
            if (type == 0x40000) return "MEM_MAPPED";
            return "UNKNOWN";
        }
        static ResidentGroup Group(Dictionary<string, ResidentGroup> dictionary, string key, string kind, string file, ulong allocation) {
            ResidentGroup group;
            if (!dictionary.TryGetValue(key, out group)) {
                group = new ResidentGroup { Kind = kind, File = file, AllocationBase = allocation == 0 ? null : "0x" + allocation.ToString("X") };
                dictionary.Add(key, group);
            }
            return group;
        }
        static void Add(ResidentGroup group, bool privatePage) {
            group.Pages++;
            if (privatePage) group.PrivatePages++;
        }
        public static ResidentSnapshot Capture(int processId) {
            if (IntPtr.Size != 8) throw new NotSupportedException("请用64位PowerShell运行此只读工具。");
            IntPtr process = OpenProcess(0x0400 | 0x0010, false, processId); // QUERY_INFORMATION | VM_READ only.
            if (process == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "无法只读打开目标进程");
            IntPtr buffer = IntPtr.Zero;
            try {
                var result = new ResidentSnapshot { ProcessId = processId };
                var processPath = new StringBuilder(32768);
                int processLength = processPath.Capacity;
                if (!QueryFullProcessImageNameW(process, 0, processPath, ref processLength)) throw new Win32Exception(Marshal.GetLastWin32Error(), "无法读取进程路径");
                result.ProcessPath = processPath.ToString();
                int capacity = 1024 * 1024;
                while (true) {
                    if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer);
                    buffer = Marshal.AllocHGlobal(capacity);
                    Marshal.WriteInt64(buffer, 0);
                    result.QueryWorkingSetAttempts++;
                    if (QueryWorkingSet(process, buffer, capacity)) break;
                    int error = Marshal.GetLastWin32Error();
                    if (error != 24) throw new Win32Exception(error, "QueryWorkingSet失败");
                    long required = Marshal.ReadInt64(buffer);
                    long next = Math.Max(capacity * 2L, (required + 1L) * 8L);
                    if (next > 64L * 1024L * 1024L) throw new InvalidOperationException("驻留页缓冲超出64MiB工具上限");
                    capacity = (int)next;
                }
                result.CapturedUtc = DateTimeOffset.UtcNow.ToString("o");
                long count = Marshal.ReadInt64(buffer);
                if (count < 0 || count > capacity / 8 - 1) throw new InvalidOperationException("驻留页数量无效");
                result.ResidentPages = count;
                var entries = new ulong[(int)count];
                for (int i = 0; i < entries.Length; i++) entries[i] = unchecked((ulong)Marshal.ReadInt64(buffer, 8 + i * 8));
                Array.Sort(entries, delegate(ulong a, ulong b) { return (a & ~4095UL).CompareTo(b & ~4095UL); });
                var categories = new Dictionary<string, ResidentGroup>();
                var files = new Dictionary<string, ResidentGroup>(StringComparer.OrdinalIgnoreCase);
                var privateAllocations = new Dictionary<string, ResidentGroup>();
                ulong regionEnd = 0;
                uint regionType = 0;
                ulong allocation = 0;
                string mapped = null;
                foreach (ulong entry in entries) {
                    ulong address = entry & ~4095UL;
                    bool privatePage = (entry & 0x100UL) == 0; // PSAPI_WORKING_SET_BLOCK.Shared bit 8.
                    if (privatePage) result.PrivateResidentPages++;
                    if (address >= regionEnd) {
                        MemoryInfo region;
                        UIntPtr bytes = VirtualQueryEx(process, new IntPtr(unchecked((long)address)), out region, (UIntPtr)Marshal.SizeOf(typeof(MemoryInfo)));
                        mapped = null;
                        if (bytes == UIntPtr.Zero) {
                            result.VirtualQueryFailures++;
                            regionType = 0;
                            allocation = 0;
                            regionEnd = address + 4096;
                        } else {
                            regionType = region.Type;
                            allocation = unchecked((ulong)region.AllocationBase.ToInt64());
                            regionEnd = unchecked((ulong)region.BaseAddress.ToInt64()) + region.RegionSize.ToUInt64();
                            if (regionType == 0x1000000 || regionType == 0x40000) {
                                var path = new StringBuilder(32768);
                                if (GetMappedFileNameW(process, new IntPtr(unchecked((long)address)), path, path.Capacity) > 0) mapped = path.ToString();
                            }
                        }
                    }
                    string kind = Kind(regionType);
                    Add(Group(categories, kind, kind, null, 0), privatePage);
                    if (regionType == 0x1000000 || regionType == 0x40000) {
                        string file = mapped ?? "(无法读取映射文件名)";
                        Add(Group(files, kind + "|" + file, kind, file, 0), privatePage);
                    } else if (regionType == 0x20000) {
                        Add(Group(privateAllocations, allocation.ToString("X"), kind, null, allocation), privatePage);
                    }
                }
                result.Categories = categories.Values.OrderByDescending(g => g.Pages).ToArray();
                result.Files = files.Values.OrderByDescending(g => g.Pages).ToArray();
                result.PrivateAllocations = privateAllocations.Values.OrderByDescending(g => g.Pages).ToArray();
                return result;
            } finally {
                if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer);
                CloseHandle(process);
            }
        }
    }
}
