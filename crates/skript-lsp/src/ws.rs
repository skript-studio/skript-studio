//! WebSocket ↔ LSP framing bridge.
//!
//! `tower-lsp::Server::new` works on an `AsyncRead + AsyncWrite` **byte**
//! stream and applies its own `LanguageCodec`, which speaks LSP's standard
//! framing (`Content-Length: N\r\n\r\n` followed by N bytes of JSON).
//!
//! WebSocket is message-oriented: each text/binary frame is exactly one
//! complete JSON-RPC message with no framing headers. So we adapt the two:
//!
//! * **Read side:** each incoming WS message (text or binary) is wrapped in
//!   a synthetic `Content-Length: N\r\n\r\n` header before being handed to
//!   `tower-lsp`'s codec.
//! * **Write side:** bytes coming out of `tower-lsp`'s codec are buffered
//!   until we have a complete `Content-Length` frame; we then strip the
//!   header and emit just the JSON body as a single WS binary message.
//!
//! Both halves use `tokio_util`-free hand-rolled poll state to avoid pulling
//! another dependency; the logic is straightforward because messages are
//! small and never span WS frames.

use futures_util::stream::Stream;
use futures_util::Sink;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// A WebSocket stream adapted to look like a byte stream for `tower-lsp`.
pub struct WsStream {
    /// Underlying WS connection.
    ws: WebSocketStream<TcpStream>,
    /// Bytes pending delivery to the LSP reader, with a synthesized
    /// `Content-Length` header prepended to each incoming message.
    read_buf: Vec<u8>,
    /// Read offset into `read_buf`.
    read_pos: usize,
    /// Outgoing bytes from `tower-lsp`'s codec, awaiting frame parsing and
    /// conversion into WS messages.
    write_buf: Vec<u8>,
}

impl WsStream {
    pub fn new(ws: WebSocketStream<TcpStream>) -> Self {
        WsStream {
            ws,
            read_buf: Vec::new(),
            read_pos: 0,
            write_buf: Vec::new(),
        }
    }

    /// Try to flush one or more complete LSP frames from `write_buf` out as
    /// WS messages.
    fn flush_write(
        cx: &mut Context<'_>,
        write_buf: &mut Vec<u8>,
        ws: &mut WebSocketStream<TcpStream>,
    ) -> Poll<io::Result<()>> {
        loop {
            // Parse one `Content-Length: N\r\n\r\n` header.
            let Some((header_end, content_len)) = parse_content_length(write_buf) else {
                // Need more bytes before we can emit a message.
                return Poll::Ready(Ok(()));
            };
            let frame_end = header_end + content_len;
            if write_buf.len() < frame_end {
                // Header seen but body incomplete.
                return Poll::Ready(Ok(()));
            }

            // Check sink readiness BEFORE consuming the frame, so a Pending
            // result leaves the buffer untouched for the next poll.
            match Pin::new(&mut *ws).poll_ready(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(e)) => {
                    return Poll::Ready(Err(io::Error::other(format!("websocket not ready: {e}"))))
                }
                Poll::Pending => return Poll::Pending,
            }

            // Now safe to consume: header + body.
            let body = write_buf[header_end..frame_end].to_vec();
            write_buf.drain(..frame_end);

            let msg = Message::Binary(body);
            if let Err(e) = Pin::new(&mut *ws).start_send(msg) {
                return Poll::Ready(Err(io::Error::other(format!("websocket send failed: {e}"))));
            }
            // Loop to handle further buffered frames; if there are none left
            // or the next is incomplete, we'll return Ready above.
        }
    }
}

impl AsyncRead for WsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        // Ensure we have something to give.
        if this.read_pos >= this.read_buf.len() {
            // Pull one WS message and stage it.
            loop {
                match Pin::new(&mut this.ws).poll_next(cx) {
                    Poll::Ready(Some(Ok(msg))) => {
                        let payload = match msg {
                            Message::Text(t) => t.into_bytes(),
                            Message::Binary(b) => b,
                            Message::Ping(_) | Message::Pong(_) => continue, // control frame
                            Message::Close(_) => return Poll::Ready(Ok(())), // clean EOF
                            Message::Frame(_) => continue,                   // shouldn't surface
                        };
                        if payload.is_empty() {
                            continue; // skip zero-length
                        }
                        this.read_buf.clear();
                        this.read_buf.extend_from_slice(
                            format!("Content-Length: {}\r\n\r\n", payload.len()).as_bytes(),
                        );
                        this.read_buf.extend_from_slice(&payload);
                        this.read_pos = 0;
                        break;
                    }
                    Poll::Ready(Some(Err(e))) => {
                        return Poll::Ready(Err(io::Error::other(format!(
                            "websocket read failed: {e}"
                        ))))
                    }
                    Poll::Ready(None) => return Poll::Ready(Ok(())), // clean EOF
                    Poll::Pending => return Poll::Pending,
                }
            }
        }
        let available = &this.read_buf[this.read_pos..];
        let n = std::cmp::min(available.len(), buf.remaining());
        buf.put_slice(&available[..n]);
        this.read_pos += n;
        if this.read_pos >= this.read_buf.len() {
            this.read_buf.clear();
            this.read_pos = 0;
        }
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for WsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.write_buf.extend_from_slice(buf);
        // Try to emit complete frames; pending is fine (bytes are buffered).
        match Self::flush_write(cx, &mut this.write_buf, &mut this.ws) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(buf.len())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Ready(Ok(buf.len())),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match Self::flush_write(cx, &mut this.write_buf, &mut this.ws) {
            Poll::Ready(Ok(())) => match Pin::new(&mut this.ws).poll_flush(cx) {
                Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
                Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(format!(
                    "websocket flush failed: {e}"
                )))),
                Poll::Pending => Poll::Pending,
            },
            other => other,
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match Self::flush_write(cx, &mut this.write_buf, &mut this.ws) {
            Poll::Ready(Ok(())) => match Pin::new(&mut this.ws).poll_close(cx) {
                Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
                Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::other(format!(
                    "websocket close failed: {e}"
                )))),
                Poll::Pending => Poll::Pending,
            },
            other => other,
        }
    }
}

/// Find the next `Content-Length: N\r\n\r\n` header at the start of `buf`.
/// Returns `(header_end_byte_offset, content_length)`.
fn parse_content_length(buf: &[u8]) -> Option<(usize, usize)> {
    // Headers end at `\r\n\r\n`.
    let header_end = find_subslice(buf, b"\r\n\r\n")?;
    let headers = &buf[..header_end];
    // Find the Content-Length line.
    for line in headers.split(|&b| b == b'\n') {
        let line = line.trim_ascii_end();
        let Some(rest) = line
            .strip_prefix(b"Content-Length:")
            .or_else(|| line.strip_prefix(b"content-length:"))
        else {
            continue;
        };
        let value = std::str::from_utf8(rest).ok()?.trim();
        let len: usize = value.parse().ok()?;
        return Some((header_end + 4, len));
    }
    None
}

/// First index of `needle` in `hay`, or `None`.
fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_content_length_basic() {
        let buf = b"Content-Length: 5\r\n\r\nhello";
        let (header_end, len) = parse_content_length(buf).unwrap();
        assert_eq!(header_end, 21); // 4 bytes of "\r\n\r\n" after "Content-Length: 5"
        assert_eq!(len, 5);
        assert_eq!(&buf[header_end..header_end + len], b"hello");
    }

    #[test]
    fn parse_content_length_missing_returns_none() {
        assert!(parse_content_length(b"hello world").is_none());
        assert!(parse_content_length(b"Content-Length: 5\r\n").is_none()); // no blank line
    }

    #[test]
    fn parse_content_length_case_insensitive() {
        let buf = b"content-length: 3\r\n\r\nabc";
        let (_, len) = parse_content_length(buf).unwrap();
        assert_eq!(len, 3);
    }

    #[test]
    fn find_subslice_works() {
        assert_eq!(find_subslice(b"abc\r\n\r\n", b"\r\n\r\n"), Some(3));
        assert_eq!(find_subslice(b"abc", b"\r\n\r\n"), None);
    }
}
