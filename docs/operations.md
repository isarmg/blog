# 运行与排障

以下命令针对[发行包安装](../deploy/linux-x86_64/README.md)的 `blog.service`，默认实例目录为 `/opt/isarmg/blog`。若使用自定义实例目录或端口，请替换示例中的值。

## 检查服务

```sh
sudo systemctl status blog.service
sudo journalctl -u blog.service -n 100 --utc --no-pager
curl -fsS http://127.0.0.1:8081/readyz
```

`/readyz` 返回 HTTP 200 和 `{"ready":true}`，表示服务能响应且 SQLite 可以执行查询，不代表代理、浏览器和上传文件均正常。另从公开地址打开首页、文章和 `/admin`，验证资源加载与登录。

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

`check` 在 `ROOT/run` 建立临时数据库快照，核验结构、完整性、业务数据和管理员凭据；它不替代备份，也不逐个检查媒体内容。服务与工具共用实例锁，不要删除锁文件绕过占用。源码开发可停服后，以实例账号执行 `./target/debug/blog-setup --root "$BLOG_ROOT" check`。

离线工具只读取 `ROOT/config/config.json`。若服务使用 `BLOG_DATA_DIR` 或 `BLOG_MEDIA` 指向外置目录，先把正确的路径写入 JSON，否则维护的可能不是正在使用的数据。见[配置参考](configuration.md)。

## 恢复管理员访问

仍能登录时使用[账号设置](usage.md#修改管理员账号)。忘记密码时，将以下 `admin` 替换为**当前**管理员用户名，新密码从提示框输入：

```sh
ROOT=/opt/isarmg/blog
sudo systemctl stop blog.service &&
  systemd-ask-password '新管理员密码：' |
    sudo "$ROOT/maintenance.sh" --root "$ROOT" admin-reset --username admin &&
  sudo systemctl start blog.service
```

重置会原子更新密码并撤销原有管理员会话，不会创建新账号或更改用户名。成功后重新登录。

## 备份与替换程序

1. 停止服务；备份 `ROOT/config/config.json`、完整的 `data_dir`（含 `site.sqlite` 及存在的 WAL 等伴随文件）和实际 `media` 目录。备份中含账号凭据和会话，只保存在受限位置。
2. 在独立目录或主机还原副本，恢复配置与运行账号的属主、权限，并准备服务账号所有的 `0700` 目录 `ROOT/logs` 和 `ROOT/run`。如果配置存有绝对数据或媒体路径，将**副本的配置**改为指向副本，避免接触生产数据。
3. 测试升级时，以实例账号运行**目标版本**已校验包中的 `libexec/blog-setup --root 副本绝对路径 check`。`maintenance.sh --root 旧实例` 只会调用旧实例的工具，不能验证目标版本是否兼容。
4. 检查通过后，在隔离的恢复实例启动匹配的服务，核对首页、管理员登录、已有文章及上传文件。`install.sh` 不提供原地升级，服务也不会自动迁移不匹配的数据库；检查失败时保留原版本与备份，不要重新初始化旧数据。
