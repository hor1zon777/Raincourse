# 雨课堂助手 v2.1.3

> 基于 Tauri v2 + Rust + React 重构的雨课堂桌面客户端，提供课程查看、答案导出与自动刷课等功能。

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB)](https://tauri.app/)
[![React](https://img.shields.io/badge/React-19-61DAFB)](https://react.dev/)
[![Rust](https://img.shields.io/badge/Rust-stable-orange)](https://www.rust-lang.org/)

## ⚠️ 免责声明

本工具**仅供个人学习、研究与技术交流使用**，请勿用于任何商业用途或非法用途。

- 使用本工具产生的所有行为（账号封禁、成绩异常、纪律处分、法律责任等）**均由使用者自行承担**，与本项目作者及贡献者无关
- 本工具与雨课堂、学堂在线及其运营方**无任何隶属关系**，不对工具的可用性、稳定性及数据准确性作出任何承诺或担保
- 使用本工具即表示您**已知悉并接受**上述条款；不同意请立即停止使用并删除本软件
- 请遵守所在国家 / 地区的法律法规及目标平台的用户协议

## 功能特性

- 🔐 **扫码登录**：通过雨课堂微信小程序扫码授权，多账号会话本地加密存储、快速切换 / 删除
- 📚 **课程管理**：课程列表、章节任务、按类型 / 名称筛选，完成情况与得分一览；页面 Tab 与勾选 / 筛选状态持久化，跨菜单切换再切回不丢失现场
- ▶️ **自动刷课**：视频 / PPT / 公告类任务自动播放进度上报；可勾选任务、**自定义视频倍速**、**多线程并行观看**；已完成任务自动跳过
- 🤖 **AI 自动答题**：章节测验 / 练习自动作答，**本地题库优先、未命中调用 OpenAI 兼容大模型兜底**；支持「试跑」预览、单个 / 批量答题；真实提交前每小题随机等待 6-9 秒，批量答题会先列出全部选中测验并逐项标记完成状态；加密字体运行时解码，无法可靠识别的题自动跳过、绝不提交脏答案；答题完成后展示作答正确率（按高 / 中 / 低分档配色）
- 📝 **答案导出**：批量获取题目与参考答案导出 Excel；作业 / 测验一键全量导出，已完成测验亦可勾选、按选中批量导出
- 🗂️ **答案文件管理**：按归属课程查看，支持单个 / 批量删除本地答案文件
- 📊 **进度面板**：总进度、用时 / 预计剩余、逐项状态、停止按钮
- 🪟 **原生体验**：Tauri v2 打包，体积小、启动快，仅 Windows NSIS 安装包

## 技术栈

| 层级 | 技术 |
| --- | --- |
| **前端** | React 19 · TypeScript · Vite · Ant Design 6 · React Router 7 · Zustand |
| **后端** | Rust · Tauri v2 · Tokio · reqwest · cookie_store |
| **AI** | OpenAI 兼容 Chat Completions（自动答题兜底）· 加密字体运行时解码 |
| **存储** | AES 加密的本地 session 文件 · JSON store |
| **打包** | Tauri Bundler · NSIS (Windows) |

## 快速开始

### 环境要求

- **Node.js** ≥ 18 + **pnpm** ≥ 9
- **Rust** stable 工具链（`rustup`）
- **Windows 11** + Microsoft Edge WebView2 Runtime（Win11 已内置）
- Tauri v2 系统依赖参见 [Tauri 官方文档](https://tauri.app/start/prerequisites/)

### 开发模式

```bash
# 安装依赖
pnpm install

# 启动开发服务器 + Tauri 窗口
pnpm tauri dev
```

> 开发模式下 Vite 监听 `127.0.0.1:5173`（已统一改为 IPv4 避免 Windows 上 `localhost` 解析卡住）。

### 构建发布版本

```bash
# 编译前端 + 打包 Windows 安装包
pnpm tauri build
```

产物：`src-tauri/target/release/bundle/nsis/雨课堂助手_<version>_x64-setup.exe`

### 其他常用脚本

```bash
pnpm dev        # 仅启动 Vite 前端 dev server
pnpm build      # 仅构建前端到 dist/
pnpm lint       # ESLint 检查
pnpm preview    # 预览前端构建产物
```

## 项目结构

```
raincourse-v2/
├── src/                          # React 前端
│   ├── routes/                   # 页面（Login/Dashboard/CourseDetail/StudyProgress/AnswerFiles/ExamExport/Settings/About）
│   ├── components/               # 通用组件（Layout 主布局、PageHeader 统一页头）
│   ├── stores/                   # Zustand 状态（authStore/courseStore）
│   ├── utils/                    # 错误归一化 / 响应守卫 / Tauri 事件
│   └── types/                    # TS 类型定义
├── src-tauri/                    # Rust 后端
│   ├── src/
│   │   ├── ai/                   # AI 自动答题（client 大模型调用 / encode 题目解析编码 / font_decode 加密字体解码 / quiz_runner 答题核心 / config 配置加密）
│   │   ├── api/                  # HTTP 客户端 + 自定义 DomainAwareJar
│   │   ├── commands/             # Tauri Command（登录/课程/导出/答案文件/自动答题/...）
│   │   ├── session/manager.rs    # 加密会话存储（多域 cookies, v2 版本）
│   │   ├── storage/json_store.rs # JSON 持久化（答案 / 考试文件读写删）
│   │   ├── ws/login.rs           # 扫码登录 WebSocket
│   │   ├── util/                 # 加密 / 文件名清洗 / 通用工具
│   │   ├── study.rs              # 自动刷课（倍速 / 并行 / 测验复用 AI 兜底）
│   │   ├── excel.rs              # Excel 导出
│   │   └── error.rs              # 统一 AppError
│   ├── assets/                   # 内嵌资源（思源黑体参考字体，用于加密字体解码）
│   ├── capabilities/default.json # Tauri 权限清单
│   ├── icons/                    # 多平台应用图标
│   └── tauri.conf.json
├── public/                       # 静态资源
└── vite.config.ts
```

## 数据与隐私

- **会话存储**：登录态保存在系统应用数据目录，使用 AES 加密，密钥派生自 data dir 路径，跨账号隔离
- **多域 Cookie**：自动按域分组持久化 `yuketang.cn` / `xuetangx.com` 等多域 cookies，避免重启后部分接口失效
- **AI 配置**：AI API Key 仅在本机加密存储、绝不上传；调用时仅向**用户自行配置的 OpenAI 兼容接口**发送题干与选项
- **无遥测**：本应用不收集、不上传任何使用数据，所有网络请求均直接发往雨课堂官方服务器或用户自配的 AI 接口

## 致谢

本项目基于原作者的开源工作进行二次开发与重构（Python → Tauri/Rust），特此致谢：

- **原作者**：[aglorice](https://github.com/aglorice)
- **原项目**：<https://github.com/aglorice/Raincourse>

## 链接

- **当前维护**：[hor1zon777](https://github.com/hor1zon777)
- **本项目地址**：<https://github.com/hor1zon777/Raincourse>
- **Issue / 反馈**：欢迎在 GitHub 提交 Issue

## 开源协议

本项目采用 **[GNU General Public License v3.0](https://www.gnu.org/licenses/gpl-3.0.html)** 协议开源发布。

**您可以**：

- ✓ 自由使用、复制、修改、分发本软件
- ✓ 将本软件用于个人与商业目的
- ✓ 基于本软件构建衍生作品

**但您必须**：

- ✓ 保留原始版权声明、许可证文本与免责声明
- ✓ 任何衍生作品同样以 GPL-3.0 协议开源（Copyleft 传染性）
- ✓ 在分发时附带完整源代码或提供源代码获取方式
- ✓ 明确标注对原始代码所做的修改

本软件按"原样"提供，不附带任何明示或暗示的担保，包括但不限于适销性、特定用途适用性的担保。完整协议文本请参阅 [LICENSE](./LICENSE) 文件或 <https://www.gnu.org/licenses/gpl-3.0.html>。
