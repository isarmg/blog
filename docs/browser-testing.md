# 浏览器验证

浏览器测试用 Playwright 驱动 Chromium 和 Firefox，用 axe 检查无障碍规则。[.node-version](../.node-version) 与 CI 固定 Node 26.7.0；[package.json](../src/web/package.json) 声明支持 `>=26.7.0 <27`，测试依赖版本由 [package-lock.json](../src/web/package-lock.json) 锁定。浏览器二进制及系统库单独安装。生产资源由 Rust 构建，测试工具只用于驱动和检查页面。

## 准备与执行

先按[开发说明](development.md)准备 Rust、WASM 目标和 Linux 编译环境，再从根目录执行：

```sh
(cd src/web && npm ci && npx playwright install --with-deps chromium firefox)
npm --prefix src/web run test:unit
env -u BLOG_SMOKE_URL npm --prefix src/web run test:browser:blog-interface
```

`npm ci` 和浏览器安装可能联网。固定版本、依赖关系及常用 API 见[测试工具说明](dependencies/testing.md)。

表中 `npm run …` 从 `src/web` 运行；其余路径相对于仓库根。

| 入口 | 检查范围 | 数据来源 |
| --- | --- | --- |
| `npm run test:unit` | 工具链合同、Rust 域测试、浏览器 fixture 单测 | 本地测试输入 |
| `npm run test:browser:blog-interface` | WASM 运行、异步状态、流取消、阅读交互和页面冒烟 | 默认使用 API fixture，不启动原生后端 |
| `npm run test:admin` | 管理布局、账号设置、幂等提交确认与表单恢复 | API fixture |
| `npm run test:public` | 公开布局、读取错误恢复、分页、语言、弹窗和匿名评论行为 | API fixture |
| `bash test/operations/smoke.sh` | 真实 HTTP、SQLite、登录、重启，以及公开页面和管理员写入 | 脚本创建临时 ROOT、启动已编译的服务，并调用浏览器 |
| CI `installed_candidate` | 候选包完整性、安装、凭据、权限与实例隔离 | 独立安装和真实后端 |

`test/browser/admin-recovery.mjs` 和 `test/browser/public-recovery.mjs` 覆盖丢失响应、原提交确认、可编辑草稿、并发分页、语言链接及原生弹窗的键盘行为。

`test/browser/pending-comment-binding.mjs` 检查会话存储中的待确认评论与当前文章或评论区绑定，拒绝缺失、错绑及未知字段，同时保留有效提交的原幂等标识。

`test/browser/editor-preview.mjs` 用真实输入和暂停时钟验证预览的 200ms 防抖、关闭与卸载取消，公开页和后台页的文档标题，以及屏外正文图片滚动到视口前后的实际请求。`test/browser/startup-failure.mjs` 验证字体失败或持续等待时页面仍可用，同时保留 WASM 加载失败、超时和手动重载恢复。

`test/browser/article-video.mjs` 在真实编辑器和公开文章中检查视频上传、正文光标插入、多播放器预览、保存与重新加载、受限平台嵌入和上传失败恢复。平台播放器请求使用本地响应，测试不依赖第三方服务；实际编码与媒体响应由视频输入和原生服务测试覆盖。

`test/browser/style-contract.mjs` 在 390px、1280px、2560px 的明暗主题中检查后台全部页面，包括有数据的表格和横向滚动操作、分页、多行字段、Markdown 预览、媒体查看器、通知与认证错误，以及下拉菜单的正常绘制和回退交互。它同时测量实际页面边距、控件尺寸、自然换行导航和 G4 轮廓状态。对比公开页改动时，先分别构建修改前和当前版本的资源，再运行 `BLOG_PUBLIC_BASELINE_DIST=/旧版/dist node test/browser/public-style-baseline.mjs`。旧版目录是必需参数；日常 CI 不跑这项对比。

`test/browser/corner-candidates.mjs` 检查公开页和管理页的正圆角尺寸均属于已登记样式角色，以及动态插入、属性和类变化、移除后的绘制状态。它还对相同 DOM 测量全元素样式发现与缓存候选的局部成本；设置 `BLOG_CORNER_BENCHMARK_OUT` 可保存结果，这项数据不代表整页渲染性能。

DOM 边界回归先运行 `bash test/interface/dom/browser/build.sh`，再执行 `node test/interface/dom/browser/helper-contracts.mjs` 与 `node test/interface/dom/browser/dom-errors.mjs`。前者通过测试专用 WASM 消费者验证计时、微任务、监听器的失败与清理，以及条件和类型擦除属性切换时旧属性与资源的移除；后者向生产 WASM 的浏览器操作注入真实异常，验证错误到达外层且启动失败能显示错误页。CI 与发行验证均执行这组双浏览器回归。

在仓库根运行 `bash test/interface/browser/build.sh` 和 `node test/interface/browser/corner-listener-lifecycle.mjs`，可通过公开圆角 API 验证 iframe 导航、停止与析构。用例覆盖同源窗口转为跨源后释放，以及浏览器移除监听器失败时的原始错误和回调生命周期。

要验证持久化，先按[开发说明](development.md)构建前后端并安装浏览器，再运行 `bash test/operations/smoke.sh`。它默认用 `${CARGO_TARGET_DIR:-target}/debug/blog`，也可用 `BLOG_SMOKE_BINARY` 指定程序。脚本自行选择回环端口，把临时实例地址传给浏览器；无需设置 `BLOG_SMOKE_URL`。单独跑 `browser-smoke.mjs` 时保持该变量未设置，否则它会访问指定实例；勿指向生产站点。

## WASM 启动检查

测试等待 `html[data-blog-runtime="blog-interface"]`，检查实际 `.wasm` 请求、`application/wasm` MIME、WASM magic 字节、资源清单中的原始 SHA-256，以及 pageerror、登录和 session 行为。

条件请求返回 304 时，需要同一 URL 先有有效 200，运行时已执行，直接 GET 可取得正确 WASM，并保存 ETag 与条件请求证据。HTML 页面可访问和 WASM 交互成功分别检查。

## CI 分组

[CI 工作流](../.github/workflows/ci.yml) 把页面测试按 `admin` 和 `public` 分组，不是按浏览器引擎拆分。各脚本选择执行的引擎；例如 `browser-smoke.mjs` 使用 Chromium，`management-native.mjs` 默认执行 Chromium 和 Firefox。聚合任务 `build-and-smoke` 要求 `server-and-smoke`、两个浏览器组及 `installed_candidate` 全部成功。构建检查、fixture 页面测试、真实后端写入和安装测试各自验证对应边界；具体命令以工作流和 `src/web/package.json` 为准。
