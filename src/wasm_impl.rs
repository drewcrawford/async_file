
//SPDX-License-Identifier: MIT OR Apache-2.0
use crate::Priority;
use std::ops::Deref;
use std::path::Path;
use std::sync::{Arc, Mutex};
use js_sys::Reflect;
use js_sys::wasm_bindgen::JsValue;
use some_executor::SomeStaticExecutor;
use some_executor::task::{Configuration, Task};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, WorkerGlobalScope, Response, ReadableStream, ReadableStreamDefaultReader};
use web_sys::wasm_bindgen::JsCast;

/**
WASM-based implementation*/

pub static FALLBACK_WASM_ORIGIN: Mutex<Option<&str>> = Mutex::new(None);

pub fn set_default_origin(or: &'static str) {
    // logwise::warn_sync!("Setting default origin to {origin}", origin=logwise::privacy::LogIt(or));
    *FALLBACK_WASM_ORIGIN.lock().unwrap() = Some(or);
}



#[derive(Debug)]
pub struct File {
    path: String,
    seek_pos: u64
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
        let path = path.as_ref().to_owned();
        let move_path = path.clone();
        let exists = Task::without_notifications("File::open".to_string(), Configuration::default(), async move {
            exists(move_path, priority).await
        }).pin_current().await;
        if !exists {
            Err(Error::NotFound)
        }
        else {
            Ok(Self {
                path: path.to_str().unwrap().to_owned(),
                seek_pos: 0,
            })
        }
    }

    pub async fn read(&self, buf_size: usize, priority: Priority) -> Result<Data, Error> {
        let seek_pos = self.seek_pos;
        let full_path = full_path(&self.path);
        let r = Task::without_notifications("File::read".to_string(), Configuration::default(), async move {
            let request_init = RequestInit::new();
            request_init.set_method("GET");
            //need to set Range: bytes=0- to read the whole file
            let map = js_sys::Map::new();
            let max_byte = seek_pos + buf_size as u64;
            map.set(&"Range".into(), &JsValue::from_str(&format!("bytes={}-{}", seek_pos,max_byte)));
            request_init.set_headers(&map.into());
            let request = Request::new_with_str_and_init(&full_path, &request_init).unwrap();
            let response = fetch_with_request(request).await?;
            if !response.ok() {
                logwise::error_sync!("Got response {status} for url {url}", status=response.status_text(), url=logwise::privacy::LogIt(full_path));
                return Err(Error::HttpStatus(response.status()));
            }
            let body = response.body().ok_or(Error::NoBody)?;
            let reader = body.get_reader();
            let default_reader: ReadableStreamDefaultReader = reader.dyn_into().unwrap();
            let mut data = Vec::with_capacity(buf_size);

            //get the 'value' property if defined
            loop {
                let read_promise = default_reader.read();
                let read_result = JsFuture::from(read_promise).await?;
                if let Some(value) = Reflect::get(&read_result, &JsValue::from_str("value")).ok() {
                    if value.is_undefined() {
                        // No more data to read
                        break;
                    }
                    //convert from Uint8Array to Vec<u8>
                    let uint8_array: js_sys::Uint8Array = value.dyn_into().unwrap();
                    let read_more = buf_size - data.len();

                    let read_more_src = uint8_array.length().min(read_more.try_into().unwrap());
                    data.extend(uint8_array.slice(0, read_more_src).to_vec());
                }
                else {
                    // No 'value' property, we assume no more data
                    break;
                }
            }
            Ok(data)
        }).pin_current().await.unwrap();

        Ok(Data(r.into_boxed_slice()))

    }

    pub async fn seek(
        &mut self,
        pos: std::io::SeekFrom,
        _priority: Priority,
    ) -> Result<u64, Error> {
        match pos {
            std::io::SeekFrom::Start(offset) => {
                self.seek_pos = offset;
                Ok(self.seek_pos)
            }
            std::io::SeekFrom::End(offset) => {
                panic!("SeekFrom::End is not supported in WASM");
            }
            std::io::SeekFrom::Current(offset) => {
                self.seek_pos = self.seek_pos
                    .checked_add(offset as u64)
                    .ok_or_else(|| Error::Wasm("SeekFrom::Current overflow".to_string()))?;
                Ok(self.seek_pos)
            }
        }
    }

    pub async fn metadata(&self, _priority: Priority) -> Result<Metadata, Error> {
        let full_path = full_path(&self.path);
        let full_path_move = full_path.clone();
        let t = Task::without_notifications("File::metadata".to_string(), Configuration::default(), async move {
            let request_init = RequestInit::new();
            request_init.set_method("HEAD");
            let request = Request::new_with_str_and_init(&full_path_move, &request_init).unwrap();

            let response = fetch_with_request(request).await.unwrap();
            if !response.ok() {
                // logwise::debuginternal_sync!("Got response {status} for url {url}", status=response.status_text(), url=logwise::privacy::LogIt(full_path));
                return Err(Error::HttpStatus(response.status()));
            }
            let headers = response.headers().get("content-length").unwrap();
            let content_length = headers
                .map(|s| s.parse::<u64>().unwrap())
                .unwrap();
            Ok(Metadata {
                len: content_length,
            })
        }).pin_current().await;
        t
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
        scope.origin().to_string()
    }
    else {
        // Fallback to a default origin if we cannot determine it
        let o = FALLBACK_WASM_ORIGIN.lock().unwrap().expect("Can't automatically determine origin.  Use set_default_origin to provide a default value here.").to_string();
        logwise::warn_sync!("Could not determine origin, using '{origin}'; to change this default use the set_default_origin function", origin=logwise::privacy::LogIt(&o));
        o
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
        let resp_value = JsFuture::from(scope.fetch_with_request(&request)).await?;
        let response: Response = resp_value.dyn_into().unwrap();
        Ok(response)
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
    // logwise::info_sync!("afile:a");
    let full_path = full_path(path);
    let r = Task::without_notifications("File::exists".to_string(), Configuration::default(), async move {
        let origin = origin();
        let opts = RequestInit::new();

        opts.set_method("HEAD");
        let request = Request::new_with_str_and_init(&full_path, &opts).unwrap();

        match fetch_with_request(request).await {
            Ok(response) => {
                if response.ok() {
                    true
                }
                else {
                    // logwise::debuginternal_sync!("Got response {status} for url {url}", status=response.status_text(), url=logwise::privacy::LogIt(full_path));
                    false
                }
            }
            Err(e) => {

                // If the request fails, we assume the file does not exist
                // logwise::debuginternal_sync!("File::exists failed for url {url}; {e}", url=logwise::privacy::LogIt(full_path),e=logwise::privacy::LogIt(e));
                false
            }
        }
    }).pin_current().await;
    r
}