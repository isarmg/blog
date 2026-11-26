# 运行与排障

以下命令针对[发行包安装](../deploy/linux-x86_64/README.md)的 `blog.service`，默认实例目录为 `/opt/isarmg/blog`。若使用自定义实例目录或端口，请替换示例中的值。

## 检查服务

```sh
sudo systemctl status blog.service
sudo journalctl -u blog.service -n 100 --utc --no-pager
curl -fsS http://127.0.0.1:8081/readyz
```

`/readyz` 返回 HTTP 200 和 `{"ready":true}` 时，服务可响应且 SQLite 查询成功。再用公开地址打开首页、带图片的文章和 `/admin`，分别检查代理、媒体和登录。

运行日志为 `ROOT/logs/server.jsonl`。每个文件上限为 8 MiB，最多保留 4 个轮转文件（`server.1.jsonl` 至 `server.4.jsonl`）。记录故障时的 UTC 时间、请求路径及响应头 `X-Request-ID`，用请求 ID 关联日志，不要收集密码或 Cookie。

| 现象 | 先检查什么 |
| --- | --- |
| 服务无法启动 | 查看 journal；核对配置文件、目录属主和权限，确认实例已初始化、数据库结构匹配且没有第二个进程占用实例锁 |
| 首页无法加载 | 确认代理将页面和 WASM、JS、CSS、字体转给同一站点，查看失败请求的状态码 |
| 后台登录或保存失败 | 使用公开 HTTPS 地址；代理保留外部 `Host`（含非默认端口）和浏览器的来源头，再检查 Cookie、会话及请求 ID |
| 写入返回 `503` | 核实服务是否正在停止、数据库是否就绪、写入队列是否已满，不要盲目重发可能已提交的写入 |
| 提交显示“尚未确认” | 在原标签页确认原提交，不要另发相同内容；见[请求约定](contracts.md#重试与幂等) |
| 文件无法删除 | 检查文章草稿、站点设置等是否仍在引用该文件，先移除引用并保存 |

## 离线检查

`maintenance.sh` 使用的是**已安装**的程序。它先校验实例中的不可变文件，再以 `isarmg-blog` 运行 `ROOT/libexec/blog-setup`。检查成功后才重启：

```sh
ROOT=/opt/isarmg/blog
sudo systemctl stop blog.service &&
  sudo "$ROOT/maintenance.sh" --root "$ROOT" check &&
  sudo systemctl start blog.service
```

每次启动服务、离线检查和密码重置都需要临时数据库快照。它把 `site.sqlite` 及已有的 `-wal`、`-journal` 复制到 `ROOT/run`，在副本上检查结构、完整性、业务数据和管理员凭据。媒体文件不参与快照。

停服维护前确认 `ROOT/run` 所在分区有足够空间。这几个数据库文件合计不能超过 4 GiB；复制并回读源文件的采集阶段共用 10 秒。磁盘慢或空间不足时，服务可能在监听端口前就启动失败，此时先看 journal。

服务与工具共用实例锁，不要删除锁文件绕过占用。源码开发可停服后，以实例账号执行 `./target/debug/blog-setup --root "$BLOG_ROOT" check`。

离线工具只读取 `ROOT/config/config.json`。若服务使用 `BLOG_DATA_DIR` 或 `BLOG_MEDIA` 指向外置目录，先把正确的路径写入 JSON，否则维护的可能不是正在使用的数据。见[配置参考](configuration.md)。

## 恢复管理员访问

仍能登录时使用[账号设置](usage.md#修改管理员账号)。忘记密码时，将以下 `admin` 替换为**当前**管理员用户名。新密码须为 12–1024 个 UTF-8 字节，不含 ASCII 控制字符；先输入再停服，取消提示或留空不会中断站点：

```sh
ROOT=/opt/isarmg/blog
(
  set +a +x
  unset blog_password
  blog_password=$(systemd-ask-password '新管理员密码：') || exit 1
  [ -n "$blog_password" ] || exit 1
  sudo systemctl stop blog.service &&
    printf '%s\n' "$blog_password" |
      sudo "$ROOT/maintenance.sh" --root "$ROOT" admin-reset --username admin &&
    sudo systemctl start blog.service
)
```

重置会原子更新密码并撤销原有管理员会话，不会创建新账号或更改用户名。成功后重新登录。

## 备份

停服后，将以下内容复制到实例目录外的受限位置；备份含管理员凭据和会话。复制完成后即可重启原站。

- `ROOT/config/config.json`。
- 完整的 `data_dir`，包括 `site.sqlite` 及现存的伴随文件。
- 实际使用的媒体目录；如需保留历史日志，也复制 `ROOT/logs`。

## 恢复与试升级

1. 在独立位置还原配置、数据和媒体；不要与原库共享硬链接。按[配置参考](configuration.md#目录与权限)恢复属主和权限，另建服务账号所有的 `0700` 目录 `ROOT/logs` 和 `ROOT/run`。
2. 检查副本 JSON：所有绝对数据、媒体路径都应指向副本。同机演练用未占用的回环端口。本机浏览器通过 HTTP 试登录时，只在副本设 `development=true`；测试生产模式则保持 `false`，使用独立的 HTTPS 地址。
3. 普通恢复用原版本发行包；试升级用**目标版本**的发行包。将已校验包放在服务账号可进入但不可修改的位置。设置 `PACKAGE` 和 `COPY_ROOT` 分别指向解压包和已恢复副本的绝对路径，运行：

   ```sh
   sudo -u isarmg-blog -- "$PACKAGE/libexec/blog-setup" --root "$COPY_ROOT" check &&
     sudo -u isarmg-blog -- "$PACKAGE/bin/blog" --root "$COPY_ROOT"
   ```

4. 该服务在前台运行。从另一终端检查副本端口的 `/readyz`，用浏览器核对管理员登录、已有文章及上传文件。演练结束按 Ctrl-C 停止副本。

`install.sh` 只做首次安装。没有原地升级命令或旧结构自动迁移；不要用 `initialize` 修复旧库。若目标版本检查失败，继续使用原版本与原数据。
