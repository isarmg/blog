# Blog 生产部署

发行包适用于 AMD64 Linux GNU/glibc。主机需有 systemd、Bash、Python 3、`useradd` 和 `runuser`；无需安装 Rust、Node 或单独的数据库服务。

## 校验发行包

从同一条 [GitHub Release](https://github.com/isarmg/blog/releases) 下载 `blog-版本号-linux-x86_64.tar.gz` 和 `SHA256SUMS`。将两者放在仅 root 可写的独立目录；先确认校验文件来自可信的发布渠道，再执行包内程序。以下命令在该目录的 root Bash 中执行：

```bash
set -euo pipefail
sha256sum --check --strict SHA256SUMS
read -r _ archive < SHA256SUMS
tar -xzpf "$archive"
cd -P -- "${archive%.tar.gz}"
```

归档内 `share/package.json` 列出程序、文档等不可变文件的摘要、大小和权限；`share/package.sha256` 是该清单的摘要。安装器会复核这些内容，但包内自检不能替代下载校验。正式包标记为 `kind=release`；`kind=candidate` 是候选构建。离线手册在 `share/README.md` 和 `share/docs/`。

## 首次安装

默认实例目录是 `/opt/isarmg/blog`。只创建它的**父目录**，不要提前创建实例目录或其中的配置。安装器会从 `--config` 读取配置、写入实例、初始化唯一管理员并检查数据库。下面直接使用包内未改动的生产示例。需要改端口时，先把示例复制到**解压目录外**，编辑副本，并把副本的绝对路径传给 `--config`。

```bash
[ -d /opt/isarmg ] || install -d -o root -g root -m 0755 /opt/isarmg
systemd-ask-password '初始管理员密码：' | ./install.sh \
  --root /opt/isarmg/blog --config "$PWD/share/config/config.json.example" --username admin
```

密码为 12–1024 个 UTF-8 字节，不含 ASCII 控制字符，只通过标准输入传递。安装器创建或验证非登录服务账号 `isarmg-blog`，写入 `blog.service` 并重新加载 systemd，但不启动或启用服务。安装成功后执行：

```bash
systemctl enable --now blog.service
curl -fsS --retry 10 --retry-connrefused --retry-delay 1 http://127.0.0.1:8081/readyz
```

使用自定义 `bind` 时相应修改探测地址。正常响应为 `{"ready":true}`。

## 接入 HTTPS

内置服务监听 HTTP。生产保持 `development=false`，由反向代理终止 TLS。使用默认回环监听时，代理将站点页面、静态资源、`/media/` 和 `/api/` 一起转给 `http://127.0.0.1:8081`，不要拆到不同的浏览器源。

代理只接收本站公开域名，将该域名和非默认端口作为上游 `Host`，保留浏览器的 `Origin` 和 `Sec-Fetch-Site`。例如从 `https://blog.example.org:8443/admin` 登录时，上游 `Host` 应是 `blog.example.org:8443`，不能改成 `127.0.0.1:8081`。仅传 `X-Forwarded-Host` 或 `X-Forwarded-Proto` 不能替代同源检查。用公开 HTTPS 地址打开首页和 `/admin`，实际登录一次。

## 安装边界与维护

- 安装器只用于首次部署。实例目录必须是包外的新绝对路径；祖先须由 root 管理，不得让其他账号替换路径。已存在的 `blog.service` 或其 drop-in 目录不会被覆盖。失败后保留部分安装供检查，不要对原目录强行重装。
- 程序归 root 所有。配置为 `root:isarmg-blog`、权限 `0640`；配置目录权限 `0750`。`state/`、`media/`、`logs/`、`run/` 由服务账号所有，权限 `0700`。生产单元启用 `PrivateTmp` 和 `ProtectHome`，不接受 `/tmp`、`/var/tmp`、`/home`、`/root` 或 `/run/user` 下的实例目录。
- 安装器只接受默认的 `ROOT/state`、`ROOT/media` 布局。手工部署外置状态或媒体目录时，须在 JSON 中指定真实路径，同时为服务账号和 systemd 单元配置相应的写入权限，见[配置参考](../../docs/configuration.md)。
- 已安装实例由 `ROOT/maintenance.sh` 校验程序并以服务账号执行离线维护，不需要保留解压目录。停机检查、重置密码和备份步骤见[运行与排障](../../docs/operations.md)，数据库版本限制见[数据与请求约定](../../docs/contracts.md)。
