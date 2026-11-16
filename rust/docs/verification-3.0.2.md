# 连点器 Modern 3.0.2 内存优化记录

## 测量环境与方法

2026年11月16—17日，同一台 Windows 11、系统构建26100、32个逻辑处理器、显示缩放150%。保留各版本正式界面和DPI策略：系统API观测原C版窗口为96 DPI、DPI unaware，Rust窗口为144 DPI、Per-monitor V2。因此下述是各版本实际产品路径的比较，不声称界面或渲染条件完全相同。

进程逐个启动、正常退出，Rust通过子进程APPDATA/LOCALAPPDATA隔离配置，保留真实系统读取。共同参数为500毫秒、F1、左键、不限次数。Common执行选择、间隔修改后恢复、下拉框展开收起、聚焦及文本读取；原C保持未锁定，不写HKLM。Rust Full另覆盖滑块、次数、方案存选删、热键弹窗取消、设置只读及关于页返回。所有自身UIA客户端退出后采20秒稳态，并以统一起点记录0/30/60秒快照和实际耗时。

WS含可共享驻留页；私有提交含可能换出的页，不能当作私有工作集。驻留分类来自QueryWorkingSet、VirtualQueryEx及映射文件名；生命周期峰值取系统PeakWorkingSetSize、PeakPagefileUsage。全过程不裁剪工作集、不强制GC。

基线快照先通过只读系统接口采集，再创建本轮UIA客户端。Common与Full各留一份功能后快照，区分按需加载和实际使用后的长期驻留。时间点使用最后一段暖机结束的同一起点，记录实际采集起止时刻，未把工具处理时间忽略后继续标成固定间隔。各轮只等待自身测试客户端退出，系统与外部辅助服务保持正常运行。

## 同条件共同操作与完整暖机

下表为各轮20秒均值的平均，单位MiB。C/R采用C、R、R、C逆序平衡；Full与Common分开解释。

| 矩阵 | 版本或变体 | 轮数 | WS | 私有提交 |
| --- | --- | ---: | ---: | ---: |
| Common | 原C 1.0 | 2 | 17.454883 | 2.524219 |
| Common | Rust 3.0.1 | 2 | 21.970801 | 3.254004 |
| Full | A：私有注册方正字体 | 4 | 24.562549 | 3.480469 |
| Full | B：仅全局方正字体 | 4 | 24.745166 | 3.517578 |
| Full | D：请求Microsoft YaHei UI | 4 | 24.925781 | 3.507764 |

Common中Rust比C多4.515918 MiB WS和0.729785 MiB私有提交。完整功能暖机的成本另外列出，未用托盘冷启动代替实际使用后的结果。

## 字体严格对照

A/B应用及运行库字节相同，唯一载荷差异是B没有私有字体文件；D只更换请求family。按ABDDBADABBAD串行完成12轮。本机全局已安装方正字体，其完整文件SHA256与随包字体相同。三个新探针进程分别未注册、私有注册、再次未注册时，四档请求的实际face均为方正公文楷体，且name/head/maxp/OS/2/hhea/cmap六表指纹全匹配；雅黑请求的实际face为Microsoft YaHei UI。

字体文件SHA256为`6118BDB12C5095246FFDFEDEDFF567E431B15CB966BCFD0B40218EA9041EF266`。字体探针在独立进程内完成注册及移除，应用实验不与有私有字体注册的探针并行，避免把探针自身的对象和缓存当成应用收益。

因此A/B测的是私有加载路径，B/D测的是family，不能把A/B称为“有字体/无字体”。四对A−B平均为WS −0.182617、私有提交−0.037109 MiB；B−D为−0.180615、+0.009814 MiB。没有据此删除或替换字体。字体磁盘体积也不能换算成进程RAM；未知映射、内核字体对象及字体服务缓存未被猜测归入字体独占成本。

## DLL驻留、Shell和GDI

以下为60秒实际驻留页均值，单位MiB。各DLL组文件集合互不重叠；其余应用/其他映像、映射和私有区另列于原始汇总，不重复相加。

| DLL组 | C Common | Rust Common | Rust Full A |
| --- | ---: | ---: | ---: |
| UIA | 1.957031 | 2.027344 | 2.184570 |
| Shell4 | 0.523438 | 1.554688 | 3.105469 |
| IconCodec | 0 | 0.710938 | 0.710938 |
| Comctl | 0 | 0.591797 | 0.632813 |
| GDI_DLL | 0.984375 | 0.917969 | 0.957031 |
| COM4 | 2.296875 | 2.792969 | 2.847656 |
| IME4 | 2.941406 | 2.941406 | 2.985352 |

UIA包含UIAutomationCore和oleacc；Shell4包含windows.storage、shell32、shlwapi、propsys；IconCodec包含IconCodecService和WindowsCodecs；Comctl为comctl32；COM4为combase、ole32、oleaut32、rpcrt4；IME4为TextInputFramework、MSCTF、CoreMessaging、CoreUIComponents。

UIA/Shell/GDI等在客户端退出后0/30/60秒仍有加载和驻留记录；loaded与resident分别记录，虚拟映像大小未当成RAM。GDI_DLL指gdi32/gdi32full/textshaping；GDI对象C为15、Rust为33，USER对象为28—29对68。这些是对象计数，不折算成内存字节。

可共享页表示页面具备共享属性，不保证此刻确有另一进程共同使用。映像私有页包含写时复制等结果，与堆内存不是同一分类。DLL组只按明确文件名归集；同一系统组件可以服务多种界面功能，表中组名不等于把其全部成本归因于单个按钮或单次API调用。

仅访问设置的两轮，Common之后平均增加WS 2.126953、私有提交0.148438 MiB。实际文件驻留增长包括windows.storage 1.058594、propsys 0.445313、SHCore 0.125000、linkinfo 0.046875 MiB；部分DLL此前已加载，此时更多页进入驻留，不能只统计新加载模块。

## 3.0.2正式改动与平衡复测

软件改动仅将ICO内PNG帧无损重编码为32位DIB及更新版本元数据。六帧RGBA逐字节相同，Windows DrawIconEx在黑白背景共12次比较均零不同像素。字体、界面及功能逻辑保留。

| 正式平衡矩阵A、P、P、A | WS均值MiB | 私有提交均值MiB |
| --- | ---: | ---: |
| A：3.0.1，两轮 | 24.537598 | 3.493750 |
| P：3.0.2，两轮 | 24.171289 | 3.358887 |

私有提交两对分别减少0.104688、0.165039 MiB，平均138.1 KiB。WS一对增加0.112305、一对减少0.844922 MiB，方向相反，不宣传稳定WS净降。P在60秒均未加载或驻留IconCodecService/WindowsCodecs，避免的两DLL驻留合计0.7109375 MiB，其中映像私有页0.03515625 MiB；此数不直接等于总WS净收益。P2实测CPU增加0.0625秒，单核平均0.312309%；其余三轮未分辨出增量，不能称为零CPU。

3.0.2通过21项单元测试、两轮完整UIA暖机，以及滚动、数字框几何居中和托盘专项；四个平衡进程均20样本完整、客户端退出与身份校验通过并正常退出，最终配置恢复一致。

正式四轮的Common、Full、退出动作分别为8、37、1个记录步骤。结束时均读回间隔500、热键F1且无修饰键、左键、无限次数、累计零、方案零。两轮新版的60秒GDI对象均33、USER均68、线程均8；对象数量与功能恢复结果一并核对。

## 指纹与复查入口

| 可执行文件 | SHA256 |
| --- | --- |
| 原C 1.0 | C790B9592D8D9A405796EC38D82BC7A571746A9D4A90774A3778D9A9004C6DCA |
| Rust 3.0.1 | 385253A7EFDA9D59F2B77B80374593E344C0552485CE83B99D9B078859B68B13 |
| Rust 3.0.2 | 5BBCBC1E93662B3AE0ADB2A8C8DB3231041D15733EFA12F312320359D793EFAF |

原始路径：`verification/warm-memory-20261116`；`analysis-generated/summary.json`、`analysis-generated/icon-balanced-independent.json`。复查工具位于`tools/warm-memory`、`tools/memory-profile`及`tools/measure.ps1`；不重叠DLL文件集合见原始目录中的`analysis-generated/original-inspect-final-common-full-shell-resident-20261117.json`。
