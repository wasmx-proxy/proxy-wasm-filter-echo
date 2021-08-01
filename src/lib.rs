#![feature(try_blocks)]
#[macro_use]
extern crate serde_derive;
mod endpoints;

use endpoints::*;
use http::StatusCode;
use log::*;
use matchit::*;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use serde::Serialize;
use url::Url;

type HttpEchoRouter = Node<HttpEchoHandler>;

struct HttpEchoRoot {}

#[no_mangle]
pub fn _start() {
    proxy_wasm::set_log_level(LogLevel::Info);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> { Box::new(HttpEchoRoot {}) });
}

impl Context for HttpEchoRoot {}
impl RootContext for HttpEchoRoot {
    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }

    fn create_http_context(&self, context_id: u32) -> Option<Box<dyn HttpContext>> {
        let res: Result<HttpEchoRouter, InsertError> = try {
            let mut root = HttpEchoRouter::new();

            // explicitly specify type of 1st element
            let endpoint: fn(&mut HttpEcho) = endpoints::send_request_anything;

            // request info
            root.insert("/anything", endpoint)?;
            root.insert("/headers", endpoints::send_request_headers)?;
            root.insert("/user-agent", endpoints::send_request_user_agent)?;

            // client echo
            root.insert("/status/:code", endpoints::echo_status)?;
            root
        };

        if res.is_err() {
            return None;
        }

        Some(Box::new(HttpEcho {
            context_id: context_id,
            data_url: None,
            router: res.unwrap(),
        }))
    }
}

struct HttpEcho {
    context_id: u32,
    data_url: Option<Url>,
    router: HttpEchoRouter,
}

impl HttpEcho {
    fn send_error_response(&mut self) {
        self.send_http_response(
            StatusCode::INTERNAL_SERVER_ERROR.as_u16() as u32,
            vec![],
            None,
        )
    }

    fn send_json_response<T>(&mut self, status: StatusCode, body: Option<T>)
    where
        T: Serialize,
    {
        if let Some(b) = body {
            match serde_json::to_string(&b) {
                Ok(s) => self.send_http_response(
                    status.as_u16() as u32,
                    vec![("Content-Type", "application/json")],
                    Some(s.as_bytes()),
                ),
                Err(_) => self.send_error_response(),
            }
        } else {
            self.send_http_response(status.as_u16() as u32, vec![], None)
        }
    }
}

impl Context for HttpEcho {}
impl HttpContext for HttpEcho {
    fn on_http_request_headers(&mut self, _: usize) -> Action {
        let url = format!(
            "{}://{}{}",
            self.get_http_request_header(":scheme").unwrap(),
            self.get_http_request_header(":authority").unwrap(),
            self.get_http_request_header(":path").unwrap()
        );

        debug!("#{} request url: {}", self.context_id, url);

        let data_url = Url::parse(url.as_str()).expect("failed to parse URL");

        if let Ok(matched) = self.router.at(data_url.path()) {
            let handler = *matched.value;
            self.data_url = Some(data_url);
            handler(self)
        } else {
            self.send_json_response::<String>(StatusCode::NOT_FOUND, None);
        }

        Action::Continue
    }
}
