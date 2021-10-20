use std::future::Future;
use std::task::{Context, Poll};
use std::pin::Pin;
use futures_core::stream::{Stream, FusedStream};
use http::response::Parts;
use http::StatusCode;
use hyper::{client::ResponseFuture, Body};
use serde::de::DeserializeOwned;
use serde_json::from_slice;

use std::cmp::min;
use std::fmt;
use crate::stream::partial_json::PartialJson;

use crate::util::{JsonStreamError, get_content_length};

/// A stream that reads a json list from a `ResponseFuture` and parses each element with
/// `serde_json`
#[must_use = "streams do nothing unless you poll them"]
pub struct JsonStream<T> {
    state: State<T>,
    capacity: usize,
    level: u32
}
enum State<T> {
    Connecting(ResponseFuture),
    Collecting(Body, PartialJson<T>),
    CollectingError(Parts, Body, Vec<u8>),
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
            State::Collecting(_, _) => f.pad("JsonStream(receiving)"),
            State::CollectingError(_, _, _) => f.pad("JsonStream(api error)"),
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
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>)
                 -> Poll<Option<Result<T, JsonStreamError>>>
    {
        let this = self.get_mut();
        let cap = this.capacity;
        let lvl = this.level;
        let state_ref = &mut this.state;
        loop {
            if let Some(poll) = state_ref.poll(cx, lvl,cap) {
                return poll;
            }
        }
    }
}

impl<T: DeserializeOwned> State<T> {
    #[inline]
    fn poll(&mut self, cx: &mut Context<'_>, lvl: u32, cap: usize)
            -> Option<Poll<Option<Result<T, JsonStreamError>>>>
    {
        match self {
            State::Connecting(ref mut fut) => {
                match Pin::new(fut).poll(cx) {
                    Poll::Pending => {
                        Some(Poll::Pending)
                    }
                    Poll::Ready(Ok(resp)) => {
                        let (parts, body) = resp.into_parts();
                        if parts.status == StatusCode::OK {
                            let json = PartialJson::new(cap, lvl);
                            *self = State::Collecting(body, json);
                        } else {
                            let size = min(get_content_length(&parts), 0x1000);
                            *self = State::CollectingError(parts, body,
                                                           Vec::with_capacity(size));
                        }
                        None
                    }
                    Poll::Ready(Err(e)) => {
                        *self = State::Done();
                        Some(Poll::Ready(Some(Err(e.into()))))
                    }
                }
            }
            State::Collecting(ref mut body, ref mut json) => match json.next() {
                Ok(Some(value)) => {
                    Some(Poll::Ready(Some(Ok(value))))
                }
                Ok(None) => {
                    match Pin::new(body).poll_next(cx) {
                        Poll::Pending => Some(Poll::Pending),
                        Poll::Ready(Some(Ok(chunk))) => {
                            json.push(&chunk[..]);
                            None
                        }
                        Poll::Ready(None) => {
                            Some(Poll::Ready(None))
                        }
                        Poll::Ready(Some(Err(e))) => {
                            *self = State::Done();
                            Some(Poll::Ready(Some(Err(e.into()))))
                        }
                    }
                }
                Err(err) => {
                    *self = State::Done();
                    Some(Poll::Ready(Some(Err(err))))
                }
            },
            State::CollectingError(ref parts, ref mut body, ref mut bytes) => {
                match Pin::new(body).poll_next(cx) {
                    Poll::Pending => Some(Poll::Pending),
                    Poll::Ready(Some(Ok(chunk))) => {
                        bytes.extend(chunk.as_ref());
                        None
                    }
                    Poll::Ready(None) => match from_slice(bytes) {
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
            State::Done() => {
                Some(Poll::Ready(None))
            }
        }
    }
}

