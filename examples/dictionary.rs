extern crate futures;
extern crate tokio_core;
extern crate tokio_proto;
extern crate tokio_service;
extern crate plaintalkio;

use std::io;
use tokio_service::Service;
use futures::{future, Future, BoxFuture};
use plaintalkio::{Message, PlainTalkProto};
use std::cell::RefCell;

use std::collections::HashMap;
pub struct DictionaryService {
    dictionary: RefCell<HashMap<Vec<u8>, Vec<u8>>>
}

impl DictionaryService {
    fn new() -> DictionaryService {
        DictionaryService {
            // TODO: what's the thread safety here?
            dictionary: RefCell::new(HashMap::new())
        }
    }
}

impl <'a> Service for DictionaryService {
    type Request = Message;
    type Response = Message;
    type Error = io::Error;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let mut fields = req.fields.iter().fuse();

        let id = fields.next();
        let cmd = fields.next().map(|bytes| bytes.as_slice());
        let word = fields.next().map(|bytes| bytes.as_slice());
        let definition = fields.next().map(|bytes| bytes.as_slice());
        let none = fields.next();

        let mut res = Message {
            fields: Vec::new()
        };

        if id == None {
            // should not be possible. We count an empty message as a single empty field
            return future::err(io::Error::new(io::ErrorKind::Other, "messages must have id field")).boxed();
        }

        res.fields.push(id.unwrap().clone());

        match (cmd, word, definition, none) {
            (Some(ref cmd), Some(ref word), None, None)
                if *cmd == b"define" =>
            {
                let dictionary = self.dictionary.borrow();
                match dictionary.get(*word) {
                    Some(definition) => {
                        res.fields.push(b"ok".to_vec());
                        res.fields.push(definition.clone());
                    }
                    None => {
                        res.fields.push(b"Word not defined".to_vec());
                    }
                }
            }
            (Some(ref cmd), Some(ref word), Some(definition), None)
                if *cmd == b"define" =>
            {
                let mut dictionary = self.dictionary.borrow_mut();
                dictionary.insert(word.to_vec(),  definition.to_vec());
                res.fields.push(b"ok".to_vec());
            }
            (Some(ref cmd), None, None, None)
                if *cmd == b"list" =>
            {
                res.fields.push(b"ok".to_vec());
                for (term, definition) in self.dictionary.borrow().iter() {
                    res.fields.push(term.clone());
                    res.fields.push(definition.clone());
                }
            }
            _ => {
                let usage = b"Messages have the following structure:
  <message-id> <command> [<argument> ...]
The following commands are available:
  help                       Get this help
  list                       List all the terms that have a definition
  define <term>              Read the definition of the given term
  define <term> <definition> Supply a definition for the given term
  Escape control characters with {}-sequences like this: O{1} HAI.";

                res.fields.push(usage.to_vec());
            }
        }

        future::ok(res).boxed()
    }
}

use tokio_proto::TcpServer;

fn main() {
    let addr = "0.0.0.0:12345".parse().unwrap();
    let server = TcpServer::new(PlainTalkProto, addr);
    server.serve(|| Ok(DictionaryService::new()));
}

