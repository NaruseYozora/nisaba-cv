# 构建与运行

当前目标为 Windows x64。需要 PowerShell 7、Git、Rust 1.99.0，以及 Visual Studio 的“使用 C++ 的桌面开发”（MSVC x64/x86、Windows SDK）。制作自解压包需 NSIS 3.x。开发脚本只设置当前进程的构建环境，不安装系统组件。

## 准备

在源码根目录执行：

```powershell
./scripts/prepare-runtime.ps1
```

脚本从 Typst 官方固定版本下载工具并验证 SHA-256，将字体复制至忽略提交的 `runtime/`。所需 VC runtime 从本机 Visual Studio 的 **Redist** 目录复制，避免从 System32 或不明下载站获取。首次准备和 Cargo 获取依赖需要联网；依赖准备完成后可使用 `-Offline` 构建。普通用户拿到完整发行包后无需下载这些工具。

## 测试与开发运行

在 Developer PowerShell，或先执行 `. ./scripts/enable-build-tools.ps1`：

```powershell
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run -p nisaba-cv --bin nisaba-cv -- --bundle runtime --data .test-output/dev-data
```

`--data` 用于显式隔离开发资料；正常发行版双击时使用程序旁的 `nisaba-data`。自动化测试与开发数据位于忽略提交的 `.test-output/`。照片和完整备份必须使用匿名样例。

Windows PDF API 用于原始历史查看。PDF 回归测试需要已准备的 runtime；无桌面或缺少 Windows PDF API 的环境不能替代正常 Windows 实机测试。

## 构建与打包

```powershell
./scripts/build.ps1
./scripts/package.ps1
./scripts/verify-package.ps1
./scripts/source-archive.ps1
```

如果 makensis 未加入 PATH：

```powershell
./scripts/package.ps1 -Makensis 'C:\Program Files (x86)\NSIS\makensis.exe'
```

`dist/Nisaba CV/` 是完整便携目录；`dist/Nisaba-CV-v1.1-windows-x64.exe` 是可上传 Releases 的自解压包。生成的 `SHA256SUMS.txt`、`manifest.json` 和 `verification.json` 用于核验。`dist/`、`runtime/` 不提交到 Git。

`source-archive.ps1` 生成可单独分享的 `dist/Nisaba-CV-v1.1-source.zip`，保留 GitHub 配置和字体，排除运行库、发行目录与测试资料。常规 GitHub 发布直接提交源码即可。

构建脚本采用静态 MSVC CRT，排版器所需 DLL 在 `tools/` 内随包提供。若目标便携目录出现 `nisaba-data`，构建和打包会拒绝，以免使用正在保存个人资料的目录。

## 开发诊断

`nisaba-cv-demo` 与 `nisaba-cv-acceptance` 只用于匿名资料和排版/性能验证，不随用户包发布。应用的 `--native-check` 与 `--self-test` 仅在显式传参时启用；后者必须给出空的 `--data` 目录，并拒绝已有资料。

发布前应检查两种字体、字号与边距极值、照片、长链接、隐藏内容、手动分页、生僻字和十页以上的页序；独立查看 PDF 文字和链接。还需要逐步补齐 Win11、干净普通用户、物理断网、真实输入法和系统 DPI 实测。
