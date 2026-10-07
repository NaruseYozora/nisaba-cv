# 维护与贡献

欢迎提交问题和改进。请先说明使用场景与复现步骤；排版问题请提供匿名文字、版式参数及预期效果。不要上传真实个人资料、数据库或备份。

## 代码结构

- `crates/resume-core/src/`：SQLite、类别、分组简历、版本、备份和资源引用。
- `crates/resume-core/migrations/`：按顺序执行的数据库升级；已发布的迁移文件不得改写，新增结构应追加迁移。
- `crates/nisaba-cv/src/`：原生界面、编辑器、PDF 排版、历史预览与字形检查。
- `crates/nisaba-cv/resume.typ`：PDF 模板；用户文字以数据形式传入。
- `assets/fonts/`：随程序提供的字库及 OFL 许可。
- `scripts/`、`packaging/`：开发准备、构建、许可汇总和便携发行。

资料库素材与简历副本需要保持独立。旧历史 PDF 必须保留原始字节。失败的保存、导出或恢复不得破坏原有资料。界面调用数据工作线程，避免在 GUI 线程等待耗时排版。

## 提交前

执行 `cargo fmt --all -- --check`、`cargo test --workspace --locked` 和 `cargo clippy --workspace --all-targets --locked -- -D warnings`。PDF 测试需要先准备 runtime。说明修改原因、验证方式及未测环境。

PDF 或字体变化还应独立检查导出文件中的文本、链接和页面边界，并目视查看照片、长文字、多页、生僻字与字号/边距极值。应用截图或合成输入事件不能替代真实鼠标和输入法验收。

## 发布

两个 crate 的版本保持一致；同时更新 README、CHANGELOG、`packaging/README.md` 和 `packaging/portable.nsi` 中的公开版本，以及 `scripts/build.ps1`、`package.ps1`、`verify-package.ps1`、`source-archive.ps1` 中的版本与文件名。更新 `Cargo.lock`，构建并验证解压后的程序，执行 `scripts/verify-package.ps1`。核对第三方许可和运行库；`dist/manifest.json` 由构建脚本生成。

默认发行目录固定为 `Nisaba CV`，版本只出现在下载包名称和 Release 标签里。不要从含 `nisaba-data` 的目录打包。GitHub Release 使用 `v1.0` 这样的标签；二进制与校验文件放在 Releases，源码提交到 Git。
