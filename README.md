# 雨课堂助手 v2

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

- 🔐 **扫码登录**：通过雨课堂微信小程序扫码授权，多账号会话本地加密存储
- 📚 **课程管理**：查看课程列表 / 章节任务 / 任务筛选
- 📝 **答案导出**：批量获取题目与参考答案，导出 Excel
- ▶️ **自动刷课**：支持视频 / PPT / 公告类任务的自动播放进度上报，可选择性勾选任务
- 📊 **进度面板**：总进度统计、停止刷课按钮
- 👤 **多账号切换**：本地加密保存登录会话，支持快速切换 / 删除已保存账号
- 🪟 **原生体验**：Tauri v2 打包，体积小、启动快，仅 Windows NSIS 安装包

## 技术栈

| 层级 | 技术 |
| --- | --- |
| **前端** | React 19 · TypeScript · Vite · Ant Design 6 · React Router 7 · Zustand |
| **后端** | Rust · Tauri v2 · Tokio · reqwest · cookie_store |
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
│   ├── routes/                   # 页面（Login/Dashboard/CourseDetail/...）
│   ├── components/Layout.tsx     # 主布局（侧边栏 + 头部）
│   ├── stores/                   # Zustand 状态（authStore/courseStore）
│   ├── utils/                    # 错误归一化 / 响应守卫 / Tauri 事件
│   └── types/                    # TS 类型定义
├── src-tauri/                    # Rust 后端
│   ├── src/
│   │   ├── api/                  # HTTP 客户端 + 自定义 DomainAwareJar
│   │   ├── commands/             # Tauri Command（登录/课程/导出/...）
│   │   ├── session/manager.rs    # 加密会话存储（多域 cookies, v2 版本）
│   │   ├── storage/json_store.rs # JSON 持久化
│   │   ├── ws/login.rs           # 扫码登录 WebSocket
│   │   ├── util/crypto.rs        # AES 加密 (data dir 派生 key)
│   │   ├── study.rs              # 自动刷课
│   │   ├── excel.rs              # Excel 导出
│   │   └── error.rs              # 统一 AppError
│   ├── capabilities/default.json # Tauri 权限清单
│   ├── icons/                    # 多平台应用图标
│   └── tauri.conf.json
├── public/                       # 静态资源
└── vite.config.ts
```

## 数据与隐私

- **会话存储**：登录态保存在系统应用数据目录（Windows: `%APPDATA%/com.raincourse.helper/`），使用 AES 加密，密钥派生自 data dir 路径，跨账号隔离
- **多域 Cookie**：自动按域分组持久化 `yuketang.cn` / `xuetangx.com` 等多域 cookies，避免重启后部分接口失效
- **无遥测**：本应用不收集、不上传任何使用数据，所有网络请求均直接发往雨课堂官方服务器

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
