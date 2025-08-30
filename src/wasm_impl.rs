
//! WebAssembly implementation of async file I/O operations.
//!
//! This module provides a WASM-compatible implementation of the async_file API by leveraging
//! the browser's Fetch API to read files served over HTTP. This allows async file operations
//! to work in web browsers and other WASM environments.
//!
//! # Architecture
//!
//! The WASM implementation treats files as HTTP resources:
//! - File paths are converted to URLs relative to the origin
//! - File reading uses HTTP GET requests with Range headers
//! - File metadata uses HTTP HEAD requests
//! - Seeking is simulated by adjusting the Range header for subsequent reads
//!
//! # Key Components
//!
//! - [`File`]: A handle representing a remote file accessed via HTTP
//! - [`Data`]: An opaque buffer containing bytes read from the file
//! - [`Metadata`]: File metadata obtained from HTTP headers
//! - [`Error`]: WASM-specific error types for file operations
//!
//! # Limitations
//!
//! - Files must be served over HTTP/HTTPS from the same origin or with proper CORS headers
//! - Write operations are not supported (read-only access)
//! - `SeekFrom::End` is not supported as it would require knowing the file size first
//! - File paths are interpreted as URLs relative to the origin
//!
//! # Origin Configuration
//!
//! The origin URL is determined automatically from the browser context (window.location.origin
//! for main thread, self.origin for workers). In environments where this cannot be determined
//! (like Node.js), use [`set_default_origin`] to configure a fallback.
//!
//!

//SPDX-License-Identifier: MIT OR Apache-2.0
use crate::Priority;
use std::ops::Deref;
use std::path::Path;
use std::sync::Mutex;
use js_sys::Reflect;
use js_sys::wasm_bindgen::JsValue;
use some_executor::task::{Configuration, Task};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, WorkerGlobalScope, Response, ReadableStreamDefaultReader};
use web_sys::wasm_bindgen::JsCast;

/// Global fallback origin URL for environments where it cannot be automatically determined.
///
/// This is used when neither `window.location.origin` nor `self.origin` are available,
/// such as in Node.js environments running WASM modules.
pub static FALLBACK_WASM_ORIGIN: Mutex<Option<&str>> = Mutex::new(None);

/// Sets the default origin URL for WASM file operations.
///
/// This function configures a fallback origin URL that will be used when the runtime
/// environment cannot automatically determine the origin. This is particularly useful
/// in Node.js or other non-browser WASM environments.
///
/// # Arguments
///
/// * `origin` - A static string slice containing the origin URL (e.g., "http://localhost:8080")
///
/// # Note
///
/// This should be called before any file operations if running in an environment
/// where the origin cannot be automatically determined.
pub fn set_default_origin(or: &'static str) {
    // logwise::warn_sync!("Setting default origin to {origin}", origin=logwise::privacy::LogIt(or));
    *FALLBACK_WASM_ORIGIN.lock().unwrap() = Some(or);
}



/// A WASM file handle for asynchronous I/O operations over HTTP.
///
/// `File` represents a remote file accessed via HTTP requests. It maintains
/// the file path and current seek position for sequential reads.
///
/// # Implementation Details
///
/// - Files are accessed using the Fetch API with appropriate headers
/// - The seek position is tracked locally and used to set Range headers
/// - Each read operation fetches only the requested byte range
///
#[derive(Debug)]
pub struct File {
    /// The path/URL of the file relative to the origin
    path: String,
    /// Current seek position in bytes from the start of the file
    seek_pos: u64
}

/// Errors that can occur during WASM file operations.
///
/// This enum represents various failure modes specific to the WASM implementation,
/// including HTTP errors and JavaScript interop issues.
///
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A general WASM or JavaScript error occurred
    #[error("WASM I/O error: {0}")]
    Wasm(String),
    /// HTTP request returned an error status code
    #[error("HTTP status code {0}")]
    HttpStatus(u16),
    /// HTTP response has no body (required for read operations)
    #[error("No body")]
    NoBody,
    /// File was not found (404 or failed HEAD request)
    #[error("Not found")]
    NotFound,
}

impl From<JsValue> for Error {
    fn from(value: JsValue) -> Self {
        Error::Wasm(format!("{:?}", value))
    }
}

/// An opaque buffer containing data read from a WASM file.
///
/// `Data` wraps a boxed byte slice containing the contents read from a file
/// via HTTP. It provides safe access to the underlying bytes through various
/// traits and methods.
#[derive(Debug)]
pub struct Data(Box<[u8]>);

/// Metadata about a WASM file obtained from HTTP headers.
///
/// `Metadata` contains information about a file retrieved via HTTP HEAD request,
/// primarily the file size from the Content-Length header.
///
#[derive(Debug, Clone)]
pub struct Metadata {
    /// The size of the file in bytes (from Content-Length header)
    len: u64,
}

impl Metadata {
    /// Returns the size of the file in bytes.
    ///
    /// This value is obtained from the Content-Length HTTP header.
    ///
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
    /// Converts this `Data` into a boxed byte slice.
    ///
    /// This method consumes the `Data` and returns the underlying `Box<[u8]>`.
    /// This is a zero-cost operation as it simply unwraps the internal storage.
    ///
    pub fn into_boxed_slice(self) -> Box<[u8]> {
        self.0
    }
    
    /// Creates a `Data` from a boxed slice (for testing)
    #[cfg(target_arch = "wasm32")]
    #[cfg(test)]
    pub fn from(bytes: Box<[u8]>) -> Self {
        Data(bytes)
    }
}

impl File {
    /// Opens a file at the given path for reading via HTTP.
    ///
    /// This method performs an HTTP HEAD request to verify the file exists before
    /// returning a `File` handle. The path is interpreted as a URL relative to
    /// the current origin.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file, relative to the origin
    /// * `priority` - The priority for this operation
    ///
    /// # Returns
    ///
    /// Returns `Ok(File)` if the file exists and is accessible, or an error if:
    /// - The file doesn't exist (404 response)
    /// - Network error occurs
    /// - CORS restrictions prevent access
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

    /// Reads up to `buf_size` bytes from the file at the current position.
    ///
    /// This method performs an HTTP GET request with a Range header to fetch
    /// only the requested bytes. The read starts at the current seek position.
    ///
    /// # Arguments
    ///
    /// * `buf_size` - Maximum number of bytes to read
    /// * `priority` - The priority for this operation
    ///
    /// # Returns
    ///
    /// Returns a `Data` object containing the bytes read. The actual number
    /// of bytes may be less than `buf_size` if:
    /// - End of file is reached
    /// - The server doesn't support range requests
    /// - Network interruption occurs
    ///
    /// # Implementation Details
    ///
    /// - Uses HTTP Range headers (e.g., `Range: bytes=0-1023`)
    /// - Reads from a `ReadableStream` using the Streams API
    /// - Accumulates chunks until `buf_size` is reached or stream ends
    pub async fn read(&self, buf_size: usize, _priority: Priority) -> Result<Data, Error> {
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

    /// Seeks to a position in the file.
    ///
    /// This method updates the internal seek position that will be used for
    /// the next read operation. The actual seeking happens lazily when the
    /// next read is performed (via the Range header).
    ///
    /// # Arguments
    ///
    /// * `pos` - The position to seek to
    /// * `priority` - The priority for this operation (currently unused)
    ///
    /// # Returns
    ///
    /// Returns the new position from the start of the file in bytes.
    ///
    /// # Limitations
    ///
    /// - `SeekFrom::End` is not supported and will panic
    /// - `SeekFrom::Current` with negative offset may cause overflow
    ///
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
            std::io::SeekFrom::End(_offset) => {
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

    /// Returns metadata about the file.
    ///
    /// This method performs an HTTP HEAD request to retrieve file metadata,
    /// primarily the file size from the Content-Length header.
    ///
    /// # Arguments
    ///
    /// * `priority` - The priority for this operation (currently unused)
    ///
    /// # Returns
    ///
    /// Returns a `Metadata` object containing the file size.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The HTTP request fails
    /// - The server returns an error status
    /// - Content-Length header is missing or invalid
    ///
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

/// Determines the origin URL for the current WASM environment.
///
/// This function attempts to determine the origin in the following order:
/// 1. From `window.location.origin` (browser main thread)
/// 2. From `self.origin` (web workers)
/// 3. From the configured fallback origin (Node.js or other environments)
///
/// # Panics
///
/// Panics if no origin can be determined and no fallback has been configured
/// via [`set_default_origin`].
///
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

/// Performs a fetch operation in the current WASM environment.
///
/// This function abstracts over different JavaScript contexts (window, worker, global)
/// to perform HTTP requests using the Fetch API.
///
/// # Arguments
///
/// * `request` - The configured `Request` object to send
///
/// # Returns
///
/// Returns the `Response` object on success, or an error if the fetch fails.
///
/// # Implementation Details
///
/// Attempts to find the fetch function in:
/// 1. `window.fetch` (browser main thread)
/// 2. `self.fetch` (web workers)
/// 3. `global.fetch` (Node.js with fetch polyfill)
///
/// # Panics
///
/// Panics if no fetch implementation is found in the global scope.
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

/// Converts a file path to a full URL by prepending the origin.
///
/// # Arguments
///
/// * `path` - The file path relative to the origin
///
/// # Returns
///
/// A complete URL string combining the origin and path.
///
fn full_path(path: impl AsRef<Path>) -> String {
    let path_str = path.as_ref().to_str().unwrap();
    let origin = origin();
    let full_path = format!("{origin}/{path_str}");
    full_path
}

/// Tests if a file exists at the given path.
///
/// This function performs an HTTP HEAD request to check if a file is accessible
/// at the given URL path. It returns `true` if the server responds with a
/// successful status code (2xx), `false` otherwise.
///
/// # Arguments
///
/// * `path` - The path to check, relative to the origin
/// * `priority` - The priority for this operation (currently unused)
///
/// # Returns
///
/// Returns `true` if the file exists and is accessible, `false` otherwise.
///
/// # Implementation Notes
///
/// - Uses HEAD request to avoid downloading file contents
/// - Returns `false` for any error (network, CORS, 404, etc.)
/// - Does not distinguish between different types of failures
pub async fn exists(path: impl AsRef<Path>, _priority: Priority) -> bool {
    // logwise::info_sync!("afile:a");
    let full_path = full_path(path);
    let r = Task::without_notifications("File::exists".to_string(), Configuration::default(), async move {
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
                logwise::debuginternal_sync!("File::exists failed for url {url}; {e}", url=logwise::privacy::LogIt(full_path),e=logwise::privacy::LogIt(e));
                false
            }
        }
    }).pin_current().await;
    r
}