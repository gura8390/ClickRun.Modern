# 第三方资源说明

## 原项目资源

Rust 版本复用同一仓库 WPF 版本的资源：

| 资源 | 原项目位置 | 用途 |
| --- | --- | --- |
| 方正公文楷体 | `../ClickRun.UI/Fonts/FZGWKaiS.ttf` | 应用字体，随程序私有加载，不安装为系统字体。 |
| 应用图标 | `../ClickRun.UI/Resources/clickrun.ico` | 可执行文件、窗口和托盘标识。 |

字体元数据包含：`Copyright(c) Beijing Founder Electronics Co.,Ltd.2015`。本次复用未附带新的字体再分发授权声明；仓库源码的 MIT 许可证不将该字体重新许可为 MIT，也不改变字体原有许可条件。

图标沿用原项目资源。原项目的源码许可见仓库根目录 [LICENSE](../LICENSE)。

## Rust 依赖

`windows`、`windows-core`、`windows-sys`、`serde` 和 `serde_json` 等依赖遵循各自项目的许可证及版权声明。锁定版本见 [Cargo.lock](Cargo.lock)，依赖声明见 [Cargo.toml](Cargo.toml)。这些依赖的许可不因纳入本程序而改变。

## 微软运行库

Windows 发布包包含微软签名的 `vcruntime140.dll`，来自 Microsoft Visual C++ 可再分发运行库。该文件适用微软对应许可条件，不包含在本项目源码的 MIT 重新许可范围内。
