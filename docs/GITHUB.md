# 将 Nisaba CV 放到 GitHub

本项目分两处发布：**仓库保存可维护的源码项目，Releases 提供可以直接运行的安装包。** 下面是首次上传的流程，使用 VS Code 连接本机 WSL Ubuntu，运行 Ubuntu 中已有的 Git。

## 实际参考过的项目

2026-10-07 查看了以下仓库的文件列表：

- [eframe_template](https://github.com/emilk/eframe_template)：本项目所用 GUI 框架的入门模板，包含 `src/`、`assets/`、Cargo 配置、README、许可、Git 忽略规则和检查脚本。
- [Psst](https://github.com/jpochyla/psst)：Rust 原生桌面应用，包含不同模块的源码、Cargo 配置、README、许可、Git 配置和打包配置；下载程序放在 Releases。
- [Xournal++](https://github.com/xournalpp/xournalpp)：桌面笔记/PDF 工具，包含源码、资源、测试、构建与打包脚本、README、许可和维护文档；下载程序放在 Releases。

这些实例说明，公开源码通常包含构建所需的配置和资源。维护文档、检查配置属于可选配套，不要求所有项目采用同样数量的目录。本项目已经准备好这些配套，本次一起提交，方便后续维护。

## 上传到代码仓库的内容

上传的是下面这些目录和文件的内容，仓库首页应直接看到 README、Cargo.toml 和 crates 等，不需要在远端再包一层 publish 或 nisaba-cv 文件夹。

| 内容 | 用途 |
| --- | --- |
| `crates/` | 真正的 Rust 源码、测试、数据库迁移与 PDF 模板；分为资料核心和界面两个模块 |
| `assets/` | 图标、字体及字体的许可；不是安装包 |
| `Cargo.toml`、`Cargo.lock`、`rust-toolchain.toml` | 项目依赖、固定依赖版本与 Rust 工具链配置 |
| `scripts/`、`packaging/`、`runtime-lock.json` | 准备排版工具、构建程序、制作与验证安装包；固定工具版本与校验值 |
| `LICENSE`、`third-party/` | MIT 许可与第三方组件许可说明 |
| `README.md`、`CHANGELOG.md`、`CONTRIBUTING.md`、`docs/` | 项目说明（包括完整 AI 声明）、版本记录、维护与构建/发布说明，另有匿名示例截图 |
| `.gitignore`、`.gitattributes` | 排除生成文件和个人资料，并统一文本换行方式 |
| `.github/` | GitHub 自动检查与问题/PR 模板；属于维护配套 |

`dist/`、`runtime/`、`.test-output/`、`target/`、`.cache/`、个人数据库和备份不提交。`.gitignore` 已配置这些规则。以下暂存命令还明确列出上传范围，因此无需删除本地构建和测试目录。

## 1. 在网站创建仓库

登录 GitHub，点击右上角 **+ → New repository**：

- 名称：`nisaba-cv`。
- 可见性：Public。
- 不额外生成 README、许可证或 gitignore，因为本地已经准备好。

创建后复制页面上的 HTTPS 仓库地址。

## 2. 在 Ubuntu 打开源码

当前维护者已整理的 Windows 路径是 `D:\简历编辑器\publish\nisaba-cv`。在 Ubuntu 终端中：

```bash
cd '/mnt/d/简历编辑器/publish/nisaba-cv'
code .
```

其他维护者应替换为自己的源码路径。VS Code 左下角应显示 `WSL: Ubuntu`；在该窗口打开终端。此流程使用 Ubuntu Git，不需要安装 Git for Windows，也不需要为上传而在 Ubuntu 编译应用。

## 3. 提交源码

以下针对首次上传。把名字和邮箱替换为自己的提交身份；如需隐藏真实邮箱，可使用 GitHub 账号 Settings → Emails 提供的 noreply 提交邮箱。

```bash
git --version
git init -b main
git config user.name '你的名字'
git config user.email '你的GitHub提交邮箱'

git add .github assets crates docs packaging scripts third-party \
  .gitignore .gitattributes Cargo.toml Cargo.lock rust-toolchain.toml \
  runtime-lock.json README.md LICENSE CHANGELOG.md CONTRIBUTING.md

git diff --cached --name-only
git status --short
```

检查这两条查看命令的输出：应只有上表中的源码、资源、配置、说明和许可，不应含安装包、运行工具或个人资料。确认后提交：

```bash
git commit -m 'Initial public release v1.0'
```

`git add` 是选择本次提交的文件，`git commit` 是保存一份本地代码记录，此时尚未上传。

## 4. 推送到 GitHub

将下面的地址换成第 1 步复制的真实仓库地址，随后执行：

```bash
git remote add origin https://github.com/你的用户名/nisaba-cv.git
git push -u origin main
```

`git push` 才会把提交上传到 GitHub。完成后刷新仓库页面，应直接看到 README 与上表所列目录。

如果已配置 GitHub HTTPS 认证，沿用现有配置。若命令行询问 Username / Password，Username 填 GitHub 用户名，Password 使用个人访问令牌，不能使用 GitHub 账号密码。可以在 GitHub **Settings → Developer settings → Personal access tokens → Fine-grained tokens** 创建只允许访问 `nisaba-cv` 的令牌：`Contents` 设为 **Read and write**；本项目包含 `.github/workflows/ci.yml`，`Workflows` 也需 **Write** 权限。令牌在密码提示处输入，不放进仓库地址或代码文件。

详见 [GitHub 本地源码上传](https://docs.github.com/en/migrations/importing-source-code/using-the-command-line-to-import-source-code/adding-locally-hosted-code-to-github)、[访问令牌说明](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens) 和 [VS Code WSL 说明](https://code.visualstudio.com/docs/remote/wsl)。

## 5. 发布可下载程序

源码上传后，进入仓库 **Releases → Draft a new release**：

1. 创建标签 `v1.0`，目标为 `main`。
2. 标题填写 `Nisaba CV v1.0`。
3. 说明可使用 `docs/RELEASE-v1.0.md` 的内容；其中“验证说明”是相对链接，粘贴到 Release 时应换成仓库 `docs/VERIFICATION.md` 的网页地址。
4. 上传 `Nisaba-CV-v1.0-windows-x64.exe` 和 `SHA256SUMS.txt`，发布 Release。

当前准备好的这两个文件在 Windows 的 `D:\简历编辑器\output\v1.0`；Ubuntu 对应 `/mnt/d/简历编辑器/output/v1.0`。其他维护者构建时，文件生成于源码目录的 `dist/`。

安装包提供给普通使用者；校验文件用于验证下载包。`Nisaba-CV-v1.0-source.zip` 是本地准备的源码副本，不需要作为代码文件上传，GitHub 会自动提供标签对应的源码下载。`manifest.json` 和 `verification.json` 是本地核对记录，本次也不必上传到 Releases。

详见 [GitHub 发布说明](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository)。

## 以后维护

继续使用同一个源码目录。修改后执行 `git add .`、`git status`、`git commit -m '说明本次修改'`、`git push`。`.gitignore` 会继续排除生成文件与个人资料。发布新版应同步提交对应源码、更新公开版本与说明，构建并验证后创建新的 Release 标签。
