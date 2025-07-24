
//SPDX-License-Identifier: MIT OR Apache-2.0
use crate::Priority;
use std::ops::Deref;
use std::path::Path;
use js_sys::Reflect;
use js_sys::wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, WorkerGlobalScope, Response, ReadableStream, ReadableStreamDefaultReader};
use web_sys::wasm_bindgen::JsCast;

/**
WASM-based implementation*/

pub static FALLBACK_WASM_ORIGIN: &str = "http://google.com";


#[derive(Debug)]
pub struct File {
    path: String,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("WASM I/O error: {0}")]
    Wasm(String),
    #[error("HTTP status code {0}")]
    HttpStatus(u16),
    #[error("No body")]
    NoBody,
    #[error("Not found")]
    NotFound,
}

impl From<JsValue> for Error {
    fn from(value: JsValue) -> Self {
        Error::Wasm(format!("{:?}", value))
    }
}

#[derive(Debug)]
pub struct Data(Box<[u8]>);

#[derive(Debug, Clone)]
pub struct Metadata {
    len: u64,
}

impl Metadata {
    pub fn len(&self) -> u64 {
        self.len
    }
}

impl AsRef<[u8]> for Data {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Deref for Data {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl Data {
    pub fn into_boxed_slice(self) -> Box<[u8]> {
        self.0
    }
}

impl File {
    pub async fn open(path: impl AsRef<Path>, priority: Priority) -> Result<Self, Error> {
        if !exists(path.as_ref(), priority).await {
            Err(Error::NotFound)
        }
        else {
            Ok(Self {
                path: path.as_ref().to_str().unwrap().to_string(),
            })
        }
    }

    pub async fn read(&self, buf_size: usize, priority: Priority) -> Result<Data, Error> {
        let request_init = RequestInit::new();
        request_init.set_method("GET");
        let full_path = full_path(&self.path);
        let request = Request::new_with_str_and_init(&full_path, &request_init).unwrap();
        let response = fetch_with_request(request).await?;
        if !response.ok() {
            logwise::debuginternal_sync!("Got response {status} for url {url}", status=response.status_text(), url=logwise::privacy::LogIt(full_path));
            return Err(Error::HttpStatus(response.status()));
        }
        let body = response.body().ok_or(Error::NoBody)?;
        let reader = body.get_reader();
        let default_reader: ReadableStreamDefaultReader = reader.dyn_into().unwrap();
        // let mut data = Vec::with_capacity(buf_size);
        let read_promise = default_reader.read();
        let read_result = JsFuture::from(read_promise).await?;
        //get the 'value' property if defined
        let value = Reflect::get(&read_result, &JsValue::from_str("value"))
            .map_err(|_| Error::Wasm("Failed to get 'value' from read result".to_string()))?;

        //convert from Uint8Array to Vec<u8>
        let uint8_array: js_sys::Uint8Array = value.dyn_into().map_err(|_| Error::Wasm("Failed to convert 'value' to Uint8Array".to_string()))?;
        //clamp the size to buf_size
        let mut vec = uint8_array.to_vec();
        if vec.len() > buf_size {
            vec.truncate(buf_size);
        }
        let data = Data(vec.into_boxed_slice());
        Ok(data)
    }

    pub async fn seek(
        &mut self,
        _pos: std::io::SeekFrom,
        _priority: Priority,
    ) -> Result<u64, Error> {
        todo!()
    }

    pub async fn metadata(&self, _priority: Priority) -> Result<Metadata, Error> {
        todo!()
    }
}

//boilerplate impls

impl PartialEq for Data {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl std::hash::Hash for Data {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

fn origin() -> String {
    let global = js_sys::global();
    if let Some(window) = web_sys::window() {
        window.location().origin().unwrap().to_string()
    }
    else if let Some(scope) = global.dyn_into::<WorkerGlobalScope>().ok() {
        todo!("Not implemented")
    }
    else {
        // Fallback to a default origin if we cannot determine it
        logwise::warn_sync!("Could not determine origin, using '{origin}'", origin=logwise::privacy::LogIt(FALLBACK_WASM_ORIGIN));
        FALLBACK_WASM_ORIGIN.to_string()
    }
}

async fn fetch_with_request(request: Request) -> Result<Response, Error> {
    let global = js_sys::global();
    if let Some(window) = web_sys::window() {
        let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
        let response: Response = resp_value.dyn_into().unwrap();
        Ok(response)
    }
    else if let Some(scope) = js_sys::global().dyn_into::<WorkerGlobalScope>().ok() {
        todo!("worker not implemented yet");
    }
    else if let Some(s) = js_sys::Reflect::get(&global, &JsValue::from_str("fetch")).ok() {
        let into = s.dyn_into::<js_sys::Function>().unwrap();
        let resp_value = into.call1(&JsValue::undefined(), &request).unwrap();
        let js_promise = resp_value.dyn_into::<js_sys::Promise>()?;
        let promise = JsFuture::from(js_promise).await?;
        let response: Response = promise.dyn_into().unwrap();
        Ok(response)

    }
    else {
        panic!("Could not find fetch in global scope");
    }
}

fn full_path(path: impl AsRef<Path>) -> String {
    let path_str = path.as_ref().to_str().unwrap();
    let origin = origin();
    let full_path = format!("{origin}/{path_str}");
    full_path
}

pub async fn exists(path: impl AsRef<Path>, _priority: Priority) -> bool {
    let origin = origin();

    let opts = RequestInit::new();
    opts.set_method("HEAD");
    let full_path = full_path(path);
    let request = Request::new_with_str_and_init(&full_path, &opts).unwrap();
    match fetch_with_request(request).await {
        Ok(response) => {
            if response.ok() {
                true
            }
            else {
                logwise::debuginternal_sync!("Got response {status} for url {url}", status=response.status_text(), url=logwise::privacy::LogIt(full_path));
                false
            }
        }
        Err(e) => {
            // If the request fails, we assume the file does not exist
            logwise::debuginternal_sync!("File::exists failed for url {url}; {e}", url=logwise::privacy::LogIt(full_path),e=logwise::privacy::LogIt(e));
            false
        }
    }

}