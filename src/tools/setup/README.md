# blog-setup

`blog-setup` 是已停止实例的离线工具。它直接调用服务端 `blog::maintenance`，与 `blog` 使用同一份配置、数据库校验和实例锁。

发行安装不需要自行构建：首次部署使用 `install.sh`；已安装实例执行 `ROOT/maintenance.sh`，由脚本校验已安装程序并切换到服务账号。见[生产部署](../../../deploy/linux-x86_64/README.md)和[运行与排障](../../../docs/operations.md)。

## 源码开发

在仓库根目录按[开发说明](../../../docs/development.md)先生成 Web 资源，再构建开发版工具：

```sh
cargo +1.100.0 build --locked --offline -p blog-setup
```

默认输出是 `./target/debug/blog-setup`，不会安装到 `PATH`。以实例账号操作，`BLOG_ROOT` 设为已准备好 `config/config.json` 的实例绝对路径。按需要选择一项，不要对已有站点执行初始化：

```sh
# 新实例：建立当前结构的数据库和管理员
systemd-ask-password '初始管理员密码：' | ./target/debug/blog-setup --root "$BLOG_ROOT" initialize --username admin

# 已停止的实例：核验数据库、管理员和私有目录
./target/debug/blog-setup --root "$BLOG_ROOT" check

# 已停止的实例：重置现有管理员的密码并撤销其会话
systemd-ask-password '新管理员密码：' | ./target/debug/blog-setup --root "$BLOG_ROOT" admin-reset --username admin
```

初始化和重置从标准输入读取单行密码，不接受密码命令行参数。`--root` 必填；可选全局参数 `--config` 只接受该实例的 `ROOT/config/config.json`。工具不应用服务的 `BLOG_DATA_DIR` 等运行时配置覆盖。
