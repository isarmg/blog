//! Product mutations and uploads: serialized work, cancellation and conflict feedback.
use super::{ApiError, LocalErrorCode, RequestOptions, enc, invalid, request, t};
use crate::contracts::UploadKind;
use blog_interface::prelude::*;
use blog_interface::thread_bound::SendWrapper;
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Clone, Copy)]
pub(crate) struct Action {
    pub busy: RwSignal<bool>,
    pub error: RwSignal<Option<ApiError>>,
    controller: StoredValue<SendWrapper<Option<web_sys::AbortController>>>,
}

impl Action {
    pub fn new() -> Self {
        let action = Self {
            busy: RwSignal::new(false),
            error: RwSignal::new(None),
            controller: StoredValue::new(SendWrapper::new(None)),
        };
        on_cleanup(move || action.abort());
        action
    }

    pub fn abort(self) {
        self.controller.update_value(|controller| {
            if let Some(controller) = (**controller).take() {
                controller.abort();
            }
        });
        let _ = self.busy.try_set(false); // Cleanup may follow disposal of the page's signals.
    }

    pub fn run(
        self,
        path: String,
        method: &'static str,
        body: Option<Value>,
        done: impl FnOnce(Value) + 'static,
    ) {
        self.run_options(path, method, body, RequestOptions::default(), done)
    }

    pub fn run_typed<T: DeserializeOwned + 'static>(
        self,
        path: String,
        method: &'static str,
        body: Option<Value>,
        done: impl FnOnce(T) + 'static,
    ) {
        self.run_typed_options(path, method, body, RequestOptions::default(), done)
    }

    pub fn run_typed_options<T: DeserializeOwned + 'static>(
        self,
        path: String,
        method: &'static str,
        body: Option<Value>,
        options: RequestOptions,
        done: impl FnOnce(T) + 'static,
    ) {
        self.run_decoded(
            path,
            method,
            body,
            options,
            blog_interface::api::decode_response,
            done,
        )
    }

    pub fn run_options(
        self,
        path: String,
        method: &'static str,
        body: Option<Value>,
        options: RequestOptions,
        done: impl FnOnce(Value) + 'static,
    ) {
        self.run_decoded(path, method, body, options, Ok, done)
    }

    fn run_decoded<T>(
        self,
        path: String,
        method: &'static str,
        body: Option<Value>,
        mut options: RequestOptions,
        decode: impl FnOnce(Value) -> Result<T, ApiError> + 'static,
        done: impl FnOnce(T) + 'static,
    ) {
        self.perform(
            move |signal| {
                options.signal = Some(signal);
                Ok(async move {
                    let response = request(&path, method, body, options)
                        .await
                        .map_err(|error| describe_conflict(&path, method, error))?;
                    decode(response)
                })
            },
            done,
        );
    }

    fn perform<T, F>(
        self,
        prepare: impl FnOnce(web_sys::AbortSignal) -> Result<F, ApiError>,
        done: impl FnOnce(T) + 'static,
    ) where
        F: Future<Output = Result<T, ApiError>> + 'static,
    {
        if self.busy.get_untracked() {
            return;
        }
        let controller = web_sys::AbortController::new()
            .expect("invariant: AbortController construction has no failure conditions");
        let signal = controller.signal();
        let operation = match prepare(signal.clone()) {
            Ok(operation) => operation,
            Err(error) => {
                self.error.set(Some(error));
                return;
            }
        };
        self.controller
            .set_value(SendWrapper::new(Some(controller)));
        self.busy.set(true);
        self.error.set(None);
        blog_interface::task::spawn_local(async move {
            let result = operation.await;
            if signal.aborted() {
                return;
            }
            match result {
                Ok(value) => done(value),
                Err(error) => self.error.set(Some(error)),
            }
            // The completion callback can cancel this action or dispose its owner.
            if !signal.aborted() {
                self.controller.set_value(SendWrapper::new(None));
                self.busy.set(false);
            }
        });
    }

    pub fn upload(self, file: SendWrapper<web_sys::File>, done: impl FnOnce(String) + 'static) {
        self.upload_file(file, UploadKind::Image, done);
    }

    pub fn upload_video(
        self,
        file: SendWrapper<web_sys::File>,
        done: impl FnOnce(String) + 'static,
    ) {
        self.upload_file(file, UploadKind::Video, done);
    }

    fn upload_file(
        self,
        file: SendWrapper<web_sys::File>,
        kind: UploadKind,
        done: impl FnOnce(String) + 'static,
    ) {
        self.perform(
            move |signal| {
                let mime = file.type_();
                if !kind.accepts(file.size(), &mime) {
                    return Err(ApiError {
                        message: match kind {
                            UploadKind::Video => t(
                                "请选择不超过 50 MB 的 MP4 或 WebM 视频",
                                "Choose an MP4 or WebM video up to 50 MB",
                            ),
                            UploadKind::Image => t(
                                "请选择不超过 10 MB 的 PNG、JPEG、GIF 或 WebP 图片",
                                "Choose a PNG, JPEG, GIF, or WebP image up to 10 MB",
                            ),
                        },
                        code: LocalErrorCode::DisplayError.into(),
                        ..invalid()
                    });
                }
                let (route, timeout_ms) = match kind {
                    UploadKind::Image => (
                        "/api/v1/content/upload",
                        blog_interface::api::DEFAULT_TIMEOUT_MS,
                    ),
                    UploadKind::Video => ("/api/v1/content/upload-video", 120_000),
                };
                let headers = vec![
                    ("Content-Type".into(), mime),
                    (
                        "X-File-Name".into(),
                        enc(crate::dates::filename_prefix(&file.name())),
                    ),
                ];
                let body = web_sys::Blob::from((*file).clone());
                Ok(async move {
                    let response = blog_interface::api::request_blob(
                        route,
                        "POST",
                        body,
                        RequestOptions {
                            signal: Some(signal),
                            timeout_ms,
                            headers,
                            ..Default::default()
                        },
                    )
                    .await?;
                    kind.uploaded_path(&response)
                        .map(str::to_owned)
                        .ok_or_else(invalid)
                })
            },
            done,
        );
    }
}

fn describe_conflict(path: &str, method: &str, mut error: ApiError) -> ApiError {
    if error.status != 409 {
        return error;
    }
    let message = if path.starts_with("/api/v1/content/resources/") {
        t(
            "文件仍被引用或记录已变化。请刷新列表，移除引用后再重试。",
            "This file is referenced or its record has changed. Refresh the list and remove its references before retrying.",
        )
    } else if method == "PUT" && path.starts_with("/api/v1/content/labels/") {
        t(
            "标签仍被文章使用，不能更改分类。请先修改文章的分类和标签。",
            "This tag is still used by articles. Update their categories and tags before moving this tag.",
        )
    } else {
        return error;
    };
    error.code = LocalErrorCode::DisplayError.into();
    error.message = message;
    error
}
