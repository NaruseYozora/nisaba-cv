# 第三方组件

Nisaba CV 自有源码采用 MIT。以下组件适用各自许可，不能统一改为 MIT。

- `assets/fonts/`：Noto / Adobe 字库的 OFL 衍生字体，原始版权、许可及修改说明位于同目录。保留原字库支持，额外 CJK 字形在 Nisaba CJK Fallback 中提供。
- Typst 0.15.1：Apache-2.0，完整 LICENSE 与 NOTICE 位于本目录。Windows 工具在开发准备时从官方固定版本下载。
- Microsoft Visual C++ runtime：微软许可，应用目录内分发，不安装系统运行库；见 `MSVC-runtime-notice.md`。发布者需确保具备相应的再分发权利。
- Rust 依赖：`Rust-dependencies.json` 与 `rust/` 中的逐组件许可。`scripts/build.ps1` 按当前锁定依赖重新汇总，并保留资源字体及 AccessKit 等附加通知。

开发准备不会把运行时工具提交到 Git；发行文件会包含这些工具及相应许可。升级依赖时请重新核对许可，特别是上游 crate 未包含在包根目录的字体或附加组件通知。
