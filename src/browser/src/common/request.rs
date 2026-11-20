//! Contract-checked reads and the page-owned state of the latest request.
use super::{page_items, t};
pub(crate) use blog_interface::api::{ApiError, ApiErrorCode, LocalErrorCode, RequestOptions};
use blog_interface::prelude::*;
use blog_interface::thread_bound::SendWrapper;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(crate) fn invalid() -> ApiError {
    ApiError {
        status: 0,
        code: LocalErrorCode::InvalidResponse.into(),
        message: t(
            "服务器响应格式无效，无法确认操作结果。",
            "The server response was invalid. The operation result could not be confirmed.",
        ),
        request_id: None,
        retryable: false,
        retry_after_seconds: None,
        source: None,
    }
}

pub(crate) async fn request(
    path: &str,
    method: &str,
    body: Option<Value>,
    options: RequestOptions,
) -> Result<Value, ApiError> {
    let v = blog_interface::api::request_with_options(path, method, body, options).await?;
    if crate::contracts::validate(path, method, &v) {
        Ok(v)
    } else {
        Err(invalid())
    }
}

struct ReadResponse<T> {
    value: T,
    current: bool,
}

impl<T> ReadResponse<T> {
    fn current(&self) -> Option<&T> {
        self.current.then_some(&self.value)
    }
}

pub(crate) struct Data<T: Send + Sync + 'static = Value> {
    response: RwSignal<Option<ReadResponse<T>>>,
    pub error: RwSignal<Option<ApiError>>,
    pub busy: RwSignal<bool>,
    pub revision: RwSignal<u64>,
}

impl<T: Send + Sync + 'static> Copy for Data<T> {}

impl<T: Send + Sync + 'static> Clone for Data<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Data<T> {
    pub fn current(self) -> Option<T>
    where
        T: Clone,
    {
        self.with_current(|value| value.cloned())
    }

    pub fn with_current<R>(self, read: impl FnOnce(Option<&T>) -> R) -> R {
        self.response
            .with(|response| read(response.as_ref().and_then(ReadResponse::current)))
    }

    pub fn with_current_untracked<R>(self, read: impl FnOnce(Option<&T>) -> R) -> R {
        self.response
            .with_untracked(|response| read(response.as_ref().and_then(ReadResponse::current)))
    }

    pub fn with_latest<R>(self, read: impl FnOnce(Option<&T>) -> R) -> R {
        self.response
            .with(|response| read(response.as_ref().map(|response| &response.value)))
    }

    pub fn update_latest(self, update: impl FnOnce(&mut T)) {
        self.response.update(|response| {
            if let Some(response) = response {
                update(&mut response.value);
            }
        });
    }

    pub fn reload(self) {
        self.revision.update(|v| *v += 1)
    }
}

impl Data {
    pub fn current_items(self) -> Option<Vec<Value>> {
        self.with_current(|value| value.map(|value| page_items(value).to_vec()))
    }
}

pub(crate) fn load(path: Signal<String>) -> Data {
    load_typed(path)
}

pub(crate) fn load_typed<T: DeserializeOwned + Send + Sync + 'static>(
    path: Signal<String>,
) -> Data<T> {
    let data = Data {
        response: RwSignal::new(None),
        error: RwSignal::new(None),
        busy: RwSignal::new(false),
        revision: RwSignal::new(0),
    };
    Effect::new(move |_| {
        let path = path.get();
        data.revision.get();
        data.response.update(|response| {
            if let Some(response) = response {
                response.current = false;
            }
        });
        data.error.set(None);
        if path.is_empty() {
            data.busy.set(false);
            return;
        }
        data.busy.set(true);
        let controller = web_sys::AbortController::new()
            .expect("invariant: AbortController construction has no failure conditions");
        let signal = controller.signal();
        let cleanup = SendWrapper::new(controller);
        on_cleanup(move || cleanup.abort());
        blog_interface::task::spawn_local(async move {
            let result = blog_interface::api::request_typed_validated_with_options::<T>(
                &path,
                "GET",
                None,
                RequestOptions {
                    signal: Some(signal.clone()),
                    max_response_bytes: 4 * 1024 * 1024,
                    ..Default::default()
                },
                |value| crate::contracts::validate(&path, "GET", value),
            )
            .await;
            if signal.aborted() {
                return;
            }
            match result {
                Ok(value) => data.response.set(Some(ReadResponse {
                    value,
                    current: true,
                })),
                Err(e) => data.error.set(Some(e)),
            }
            data.busy.set(false);
        });
    });
    data
}
