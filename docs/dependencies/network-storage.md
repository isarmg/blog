# 网络与存储

接口索引：[network](catalog.md#network)、[storage](catalog.md#storage)、[native](catalog.md#native)。

## HTTP

HTTP 值、Body、传输、请求流水线、路由和中间件分层保存。`http-message` 提供共享请求/响应类型，`body-stream` 处理流式 Body，`http-transport` 承担连接，`request-router` 组织路由和提取器。HTTP 日期和字节范围解析同属 `http-message`；地址、媒体类型、表单分别由 `web-address`、`media-type`、`form-data` 提供。

路由指南保存在源码旁，并参与 Rust 文档生成：

- [处理函数](../../src/foundation/network/request-router/src/docs/handlers_intro.md)、[提取器](../../src/foundation/network/request-router/src/docs/extract.md)、[响应](../../src/foundation/network/request-router/src/docs/response.md)。
- [错误处理](../../src/foundation/network/request-router/src/docs/error_handling.md)、[中间件](../../src/foundation/network/request-router/src/docs/middleware.md)、[处理函数类型错误](../../src/foundation/network/request-router/src/docs/debugging_handler_type_errors.md)。
- [路由](../../src/foundation/network/request-router/src/docs/routing/route.md)、[嵌套路由](../../src/foundation/network/request-router/src/docs/routing/nest.md)、[状态](../../src/foundation/network/request-router/src/docs/routing/with_state.md)。

服务端和资源开发代理复用同一套 HTTP 类型与传输。开发代理使用 HTTP/1 连接池，保持请求和响应的流式背压；超时覆盖整个转发生命周期。Cookie、CSRF、重定向和逐跳头处理位于产品入口，不能依靠通用客户端的隐式重试。服务使用自己的 HTTP/1 监听器；产品引入路由包时关闭默认特性，JSON 与查询提取按调用方启用。

## SQLite

存储层只服务 SQLite。`sqlite-native` 编译仓库内 SQLite C 源码并导出绑定；`sqlite-access` 提供 Tokio 池、事务、预处理语句、值编解码与行访问；`record-derive` 为读取模型生成 `FromRow` 映射。服务端按需启用 JSON 编解码和派生宏，不依赖 SQL 层的 chrono 时间转换。

[访问层 API 概览](../../src/foundation/storage/sqlite-access/src/lib.md)与本地 rustdoc 说明接口。连接池、忙等待和事务取消的行为应按实际调用处理。行可移动到另一线程，其内部解码状态不允许并发共享。

数据库身份与初始化规则见[产品合同](../contracts.md)；实例路径、备份和故障排查见[运维](../operations.md)。这些业务约束由应用层实现，不由通用 SQL 类型推断。

## 原生构建

`native/target-conditions` 提供 Rust 条件编译宏。SQLite 的 [构建脚本](../../src/foundation/storage/sqlite-native/build.rs) 调用该包内的 [C 编译辅助代码](../../src/foundation/storage/sqlite-native/build/compiler.rs)，编译仓库内的 `sqlite3.c` 并生成静态归档，不链接外部 SQLite。宿主仍需 AMD64 Linux GNU/glibc 的 C 编译器、系统头文件、归档工具和链接器。
