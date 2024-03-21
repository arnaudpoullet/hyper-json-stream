use futures_core::stream::{FusedStream, Stream};
use http::response::Parts;
use http::StatusCode;
use serde::de::DeserializeOwned;
use std::ffi::{c_int, c_uint};
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};

use crate::ffi::{zalloc, zfree};
use crate::stream::partial_json::PartialJson;
use hyper::body::{Body, Incoming};
use hyper_util::client::legacy::ResponseFuture;
use libz_sys as zlib;
use std::cmp::{self, min};
use std::io::ErrorKind;
use std::{fmt, io, mem, ptr};

use crate::util::{get_content_length, JsonStreamError};

use super::encoding::ContentEncoding;

/// A stream that reads a json list from a `ResponseFuture` and parses each element with
/// `serde_json`
#[must_use = "streams do nothing unless you poll them"]
pub struct JsonStream<T> {
    state: State<T>,
    capacity: usize,
    level: u32,
}
enum State<T> {
    Connecting(ResponseFuture),
    Collecting {
        body: Incoming,
        json: PartialJson<T>,
        encoding: ContentEncoding,
        stream: *mut zlib::z_stream,
        total_in: u64,
    },
    CollectingError(Parts, Incoming, Vec<u8>),
    EncodingError(),
    Done(),
}
// The ResponseFuture does not implement Sync, but since it can only be accessed through
// &mut methods, it is not possible to synchronously access it.
unsafe impl<T> Sync for State<T> {}
// The compiler adds a T: Send bound, but it is not needed as we don't store any Ts.
unsafe impl<T> Send for State<T> {}
// The compiler adds a T: Unpin bound, but it is not needed as we don't store any Ts.
impl<T> Unpin for State<T> {}

impl<T> fmt::Debug for JsonStream<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.state {
            State::Connecting(_) => f.pad("JsonStream(connecting)"),
            State::Collecting { .. } => f.pad("JsonStream(receiving)"),
            State::CollectingError(_, _, _) => f.pad("JsonStream(api error)"),
            State::EncodingError() => f.pad("JsonStream(encoding error)"),
            State::Done() => f.pad("JsonStream(done)"),
        }
    }
}

impl<T: DeserializeOwned> JsonStream<T> {
    /// Create a new `JsonStream`. The `capacity` is the initial size of the allocation
    /// meant to hold the body of the response.
    pub fn new(resp: ResponseFuture, level: u32, capacity: usize) -> Self {
        JsonStream {
            state: State::Connecting(resp),
            capacity,
            level,
        }
    }
}
impl<T: DeserializeOwned> FusedStream for JsonStream<T> {
    /// Returns `true` if this stream has completed.
    fn is_terminated(&self) -> bool {
        matches!(self.state, State::Done())
    }
}
impl<T: DeserializeOwned> Stream for JsonStream<T> {
    type Item = Result<T, JsonStreamError>;
    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<T, JsonStreamError>>> {
        let this = self.get_mut();
        let cap = this.capacity;
        let lvl = this.level;
        let state_ref = &mut this.state;
        loop {
            if let Some(poll) = state_ref.poll(cx, lvl, cap) {
                return poll;
            }
        }
    }
}

impl<T: DeserializeOwned> State<T> {
    #[inline]
    fn poll(
        &mut self,
        cx: &mut Context<'_>,
        lvl: u32,
        cap: usize,
    ) -> Option<Poll<Option<Result<T, JsonStreamError>>>> {
        match self {
            State::Connecting(ref mut fut) => match Pin::new(fut).poll(cx) {
                Poll::Pending => Some(Poll::Pending),
                Poll::Ready(Ok(resp)) => {
                    let (parts, body) = resp.into_parts();
                    let content_encoding_opt = parts.headers.get("Content-Encoding");
                    let encoding = if let Some(content_encoding) = content_encoding_opt {
                        let content_encoding_str = content_encoding.to_str().unwrap();
                        ContentEncoding::from_str(content_encoding_str).unwrap()
                    } else {
                        ContentEncoding::None
                    };
                    match parts.status {
                        StatusCode::OK => {
                            let json = PartialJson::new(cap, lvl);
                            if encoding == ContentEncoding::Gzip {
                                let stream = Box::into_raw(Box::new(zlib::z_stream {
                                    next_in: ptr::null_mut(),
                                    avail_in: 0,
                                    total_in: 0,
                                    next_out: ptr::null_mut(),
                                    avail_out: 0,
                                    total_out: 0,
                                    msg: ptr::null_mut(),
                                    adler: 0,
                                    data_type: 0,
                                    reserved: 0,
                                    opaque: ptr::null_mut(),
                                    state: ptr::null_mut(),
                                    zalloc,
                                    zfree,
                                }));
                                let res = unsafe {
                                    zlib::inflateInit2_(
                                        stream,
                                        47,
                                        zlib::zlibVersion(),
                                        mem::size_of::<zlib::z_stream>() as c_int,
                                    )
                                };

                                if res == zlib::Z_OK {
                                    *self = State::Collecting {
                                        body,
                                        json,
                                        encoding,
                                        stream,
                                        total_in: 0,
                                    };
                                } else {
                                    *self = State::EncodingError();
                                }
                            } else {
                                *self = State::Collecting {
                                    body,
                                    json,
                                    encoding,
                                    stream: ptr::null_mut(),
                                    total_in: 0,
                                };
                            }
                        }
                        StatusCode::NO_CONTENT => *self = State::Done(),
                        _ => {
                            let size = min(get_content_length(&parts), 0x1000);
                            *self = State::CollectingError(parts, body, Vec::with_capacity(size));
                        }
                    }
                    None
                }
                Poll::Ready(Err(e)) => {
                    *self = State::Done();
                    Some(Poll::Ready(Some(Err(e.into()))))
                }
            },
            State::Collecting {
                ref mut body,
                ref mut json,
                ref encoding,
                ref stream,
                ref mut total_in,
                ..
            } => match json.next() {
                Ok(Some(value)) => Some(Poll::Ready(Some(Ok(value)))),
                Ok(None) => match Pin::new(body).poll_frame(cx) {
                    Poll::Pending => Some(Poll::Pending),
                    Poll::Ready(Some(Ok(chunk))) => match chunk.into_data() {
                        Ok(b) => {
                            if *encoding == ContentEncoding::None {
                                json.push(&b[..]);
                            } else {
                                let mut bytes_vec = b.to_vec();
                                loop {
                                    let mut output_buffer = [0; 1024];
                                    let data = &mut bytes_vec[*total_in as usize..];
                                    let inflate_res = unsafe {
                                        (*(*stream)).next_in = data.as_mut_ptr();
                                        (*(*stream)).avail_in =
                                            cmp::min(b.len(), c_uint::MAX as usize) as c_uint;
                                        (*(*stream)).total_in = *total_in;
                                        (*(*stream)).next_out = output_buffer.as_mut_ptr();
                                        (*(*stream)).avail_out =
                                            cmp::min(output_buffer.len(), c_uint::MAX as usize)
                                                as c_uint;

                                        zlib::inflate(*stream, zlib::Z_NO_FLUSH)
                                    };

                                    if inflate_res == zlib::Z_BUF_ERROR || inflate_res == zlib::Z_OK
                                    {
                                        unsafe {
                                            *total_in = (*(*stream)).total_in;
                                            if (*(*stream)).total_in as usize >= b.len() {
                                                break;
                                            }
                                        }

                                        json.push(&output_buffer);
                                    } else {
                                        eprintln!("zlib::inflate returned {}", inflate_res);
                                        return Some(Poll::Ready(Some(Err(
                                            JsonStreamError::EncodingError(
                                                "Failed to decode bytes".to_string(),
                                            ),
                                        ))));
                                    }
                                }

                                *total_in = 0;
                            }

                            None
                        }
                        Err(fr) => {
                            eprintln!("{:?}", fr);
                            Some(Poll::Ready(Some(Err(JsonStreamError::IOError(
                                io::Error::new(
                                    ErrorKind::InvalidData,
                                    "Could not get bytes from frame",
                                ),
                            )))))
                        }
                    },
                    Poll::Ready(None) => Some(Poll::Ready(None)),
                    Poll::Ready(Some(Err(e))) => {
                        *self = State::Done();
                        Some(Poll::Ready(Some(Err(e.into()))))
                    }
                },
                Err(err) => {
                    *self = State::Done();
                    Some(Poll::Ready(Some(Err(err))))
                }
            },
            State::CollectingError(ref parts, ref mut body, ref mut bytes) => {
                match Pin::new(body).poll_frame(cx) {
                    Poll::Pending => Some(Poll::Pending),
                    Poll::Ready(Some(Ok(chunk))) => match chunk.into_data() {
                        Ok(b) => {
                            bytes.extend(b.as_ref());
                            None
                        }
                        Err(fr) => {
                            eprintln!("{:?}", fr);
                            Some(Poll::Ready(Some(Err(JsonStreamError::IOError(
                                io::Error::new(
                                    ErrorKind::InvalidData,
                                    "Could not get bytes from frame",
                                ),
                            )))))
                        }
                    },
                    Poll::Ready(None) => match String::from_utf8(bytes.clone()) {
                        Ok(err_msg) => {
                            let err = JsonStreamError::ApiError(parts.status, err_msg);
                            *self = State::Done();
                            Some(Poll::Ready(Some(Err(err))))
                        }
                        Err(err) => {
                            *self = State::Done();
                            Some(Poll::Ready(Some(Err(err.into()))))
                        }
                    },
                    Poll::Ready(Some(Err(err))) => {
                        *self = State::Done();
                        Some(Poll::Ready(Some(Err(err.into()))))
                    }
                }
            }
            State::EncodingError() => Some(Poll::Ready(Some(Err(JsonStreamError::EncodingError(
                "Failed to decode the payload with gzip".to_string(),
            ))))),
            State::Done() => Some(Poll::Ready(None)),
        }
    }
}
