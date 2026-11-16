using System.Diagnostics;
using System.Text.Json;
using ClickRun.Core.Models;
using ClickRun.Core.Services;

// Uses the shipped, unmodified 2.1.1 core; the sender returns success without SendInput.
int interval = int.Parse(args[0]);
int seconds = int.Parse(args[1]);
var settings = new ClickConfig { Interval = interval, Mode = ClickMode.Single, RandomDelay = 0, ClickLimit = 0 };
using (var warm = new ClickService(_ => true))
{
    warm.Start(new ClickConfig { Interval = 1 }, new ClickStats());
    Thread.Sleep(500);
    warm.Stop();
}
using var process = Process.GetCurrentProcess();
using var engine = new ClickService(_ => true);
var stats = new ClickStats();
Console.WriteLine("READY");
Console.Out.Flush();
if (Console.ReadLine() != "START") return 2;
process.Refresh();
double before = process.TotalProcessorTime.TotalSeconds;
var watch = Stopwatch.StartNew();
engine.Start(settings, stats);
Thread.Sleep(TimeSpan.FromSeconds(seconds));
engine.Stop();
watch.Stop();
process.Refresh();
double after = process.TotalProcessorTime.TotalSeconds;
Console.WriteLine(JsonSerializer.Serialize(new {
    implementation = "Modern 2.1.1 shipped .NET core",
    workload = "dry sender; single click; zero random; unlimited; no UI/config/autostart",
    intervalMs = interval,
    requestedSeconds = seconds,
    elapsedSeconds = watch.Elapsed.TotalSeconds,
    clicks = stats.SessionClicks,
    clicksPerSecond = stats.SessionClicks / watch.Elapsed.TotalSeconds,
    cpuSeconds = after - before,
    cpuPercentOneCore = 100 * (after - before) / watch.Elapsed.TotalSeconds,
    cpuMillisecondsPerThousandClicks = stats.SessionClicks == 0 ? 0 : (after - before) * 1_000_000 / stats.SessionClicks,
    runtime = System.Runtime.InteropServices.RuntimeInformation.FrameworkDescription,
    processId = Environment.ProcessId
}));
return 0;
