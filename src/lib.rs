extern crate futures;
extern crate tokio_core;
extern crate tokio_proto;

use std::io;
use std::usize;
use tokio_core::io::{Codec, EasyBuf};
use tokio_core::io::{Io, Framed};

pub struct Message {
    pub fields: Vec<Vec<u8>>
}

// ---------------------------- CODEC ------------------------------

fn is_safe_byte (byte: u8) -> bool {
    byte != b'\r' && byte != b'\n' && byte != b' ' && byte != b'{'
}

pub struct PlainTalkCodec;

impl Codec for PlainTalkCodec {
    type In = Message;
    type Out = Message;

    fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<Self::In>> {
        let mut fields = Vec::new();
        let mut current_field = Vec::new();
        let mut next_byte_must_be_newline = false;

        let mut i = 0;
        while i < buf.len() {
            let byte = *buf.as_slice().get(i).unwrap();

            if next_byte_must_be_newline && byte != b'\n' {
                return Err(io::Error::new(io::ErrorKind::Other, "expected \\n after \\r"));
            }

            match byte {
                b' ' => {
                    fields.push(current_field);
                    current_field = Vec::new();
                }
                b'\n' => {
                    fields.push(current_field);
                    buf.drain_to(i + 1);
                    return Ok(Some(Message { fields: fields }));
                }
                b'\r' => {
                    next_byte_must_be_newline = true;
                }
                b'{' => {
                    let mut esc_sequence_length : usize = 0;
                    i += 1;
                    while i < buf.len() {
                        let byte = *buf.as_slice().get(i).unwrap();
                        if byte < b'0' || byte > b'9' {
                            break;
                        }

                        let (n, overflow) = esc_sequence_length.overflowing_mul(10);
                        if overflow {
                            return Err(io::Error::new(io::ErrorKind::Other, "escape sequence too long"));
                        }
                        let x = (byte - b'0') as usize;
                        let (n, overflow) = n.overflowing_add(x);
                        if overflow {
                            return Err(io::Error::new(io::ErrorKind::Other, "escape sequence too long"));
                        }

                        esc_sequence_length = n;
                        i += 1;
                    }
                    if i >= buf.len() {
                        return Ok(None);
                    }
                    if *buf.as_slice().get(i).unwrap() != b'}' {
                        return Err(io::Error::new(io::ErrorKind::Other, "escape sequence length containing illegal characters"));
                    }
                    i += 1;

                    // TODO: probably check for reasonable/configurable max value of escape sequence

                    let (new_i, overflow) = i.overflowing_add(esc_sequence_length);
                    if overflow || new_i >= buf.len() {
                        return Ok(None);
                    }

                    current_field.extend(buf.as_slice()[i..new_i].iter());
                    i = new_i;
                    continue;
                }
                _ => {
                    current_field.push(byte);
                }
            }

            i += 1;
        }
        Ok(None)
    }

    fn encode(&mut self, msg: Message, buf: &mut Vec<u8>) -> io::Result<()>
    {
        // allocate the minimum required size. Might grow beyond that
        let min_size = msg.fields.iter().map(|field| field.len() + 1).sum(); // +1 for field separator (space or new line)
        buf.reserve(min_size);

        let mut is_first = true;
        for field in msg.fields {
            if !is_first {
                buf.push(b' ');
            }
            is_first = false;

            let any_unsafe = field.iter().any(|byte| !is_safe_byte(*byte));

            if any_unsafe {
                // prepend escaping length on the whole field
                // if there are any unsafe bytes
                buf.push(b'{');
                buf.extend(field.len().to_string().bytes());
                buf.push(b'}');
            }

            buf.extend(field);
        }
        buf.push(b'\n');
        Ok(())
    }
}

// ---------------------------- PROTOCOL ------------------------------

use tokio_proto::pipeline::ServerProto;
pub struct PlainTalkProto;

impl<T: Io + 'static> ServerProto<T> for PlainTalkProto {
    type Request = Message;
    type Response = Message;

    /// A bit of boilerplate to hook in the codec:
    type Transport = Framed<T, PlainTalkCodec>;
    type BindTransport = Result<Self::Transport, io::Error>;
    fn bind_transport(&self, io: T) -> Self::BindTransport {
        Ok(io.framed(PlainTalkCodec))
    }
}

