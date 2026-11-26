# 安装与首次运行

Blog 服务端运行于 AMD64 Linux GNU/glibc；读者只需支持 WebAssembly 的浏览器。

| 场景 | 第一步 |
| --- | --- |
| 生产部署 | 从同一条 [GitHub Release](https://github.com/isarmg/blog/releases) 下载 Linux x86_64 安装包及 `SHA256SUMS`，按[部署说明](../deploy/linux-x86_64/README.md)校验、安装和启动 |
| 本机开发 | 按[开发说明](development.md)构建 Web 资源和两个原生程序，创建开发配置，执行 `blog-setup initialize` 并启动服务 |

两种方式都使用独立的绝对目录 `ROOT`。**安装包**要求这个目录尚不存在，配置作为 `install.sh --config` 的输入，由安装器写入 `ROOT/config/config.json`。**源码运行**则由你先创建配置，再初始化新站点。管理员密码从标准输入读取；已有站点不要重新初始化。

## 验证站点

1. 在运行服务的机器上请求 `GET /readyz`。发行包默认地址是 `http://127.0.0.1:8081/readyz`；按开发示例启动则用 `http://127.0.0.1:18091/readyz`。就绪时返回 HTTP 200 和 `{"ready":true}`。
2. 浏览器打开首页。生产使用对外的 **HTTPS 地址**；本机开发示例使用 `http://127.0.0.1:18091/`。`development=true` 只允许回环 HTTP。
3. 在同一地址打开 `/admin`，使用初始化时设置的账号登录。按[使用指南](usage.md)设置站点信息，发布并从前台打开第一篇文章。

端口和权限见[配置参考](configuration.md)，已有实例的检查、备份和密码恢复见[运行与排障](operations.md)。
