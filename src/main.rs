// main.rs

#![feature(await_macro, async_await)]

use tokio::await;
use tokio::prelude::*;

use failure::{err_msg, Fail};
use futures::{future, future::Either, Future};
use hyper::client::connect::{Destination, HttpConnector};
use tower_grpc::Request;
use tower_hyper::client::ConnectError;
use tower_hyper::{client, util};
use tower_util::MakeService;

use crate::etcdserverpb::client::Kv;
use crate::etcdserverpb::RangeRequest;

pub mod mvccpb {
    include!(concat!(env!("OUT_DIR"), "/mvccpb.rs"));
}

pub mod authpb {
    include!(concat!(env!("OUT_DIR"), "/authpb.rs"));
}

pub mod etcdserverpb {
    include!(concat!(env!("OUT_DIR"), "/etcdserverpb.rs"));
}

type HTTPConn = tower_request_modifier::RequestModifier<
    tower_hyper::client::Connection<tower_grpc::BoxBody>,
    tower_grpc::BoxBody,
>;

async fn run_kvclient(ip: &str, port: u16) -> Result<(), failure::Error> {
    let mut client = await!(KvClient::new(ip, port))?;
    let resp = await!(client.get_string("hello"))?;
    println!("RESP=>{:?}", resp);

    Ok(())
}

#[tokio::main]
async fn main() {
    let _ = ::env_logger::init();

    let host = "127.0.0.1";
    let port = 2379;

    match await!(run_kvclient(host, port)) {
        Ok(_) => println!("done."),
        Err(e) => eprintln!("run kvclient failed; error = {:?}", e),
    }
}

#[derive(Debug, Fail)]
pub enum EtcdClientError {
    #[fail(display = "connect error: {}", _0)]
    Connect(ConnectError<std::io::Error>),
    #[fail(display = "error message: {}", _0)]
    ErrMsg(String),
}

struct KvClient {
    inner: Kv<HTTPConn>,
}

impl KvClient {
    pub fn new(host: &str, port: u16) -> impl Future<Item = KvClient, Error = EtcdClientError> {
        let uri: http::Uri = match format!("http://{}:{}", host, port).parse() {
            Ok(uri) => uri,
            Err(e) => {
                return Either::A(future::err(EtcdClientError::ErrMsg(format!(
                    "parse uri failed, {:?}",
                    e
                ))))
            }
        };

        let dst = match Destination::try_from_uri(uri.clone()) {
            Ok(dst) => dst,
            Err(e) => {
                return Either::A(future::err(EtcdClientError::ErrMsg(format!(
                    "build dst from uri failed, {:?}",
                    e
                ))))
            }
        };

        let connector = util::Connector::new(HttpConnector::new(4));
        let settings = client::Builder::new().http2_only(true).clone();
        let mut make_client = client::Connect::new(connector, settings);

        Either::B(
            make_client
                .make_service(dst)
                .map(move |conn| {
                    let conn = tower_request_modifier::Builder::new()
                        .set_origin(uri)
                        .build(conn)
                        .unwrap();

                    KvClient {
                        inner: Kv::new(conn),
                    }
                })
                .map_err(|e| EtcdClientError::ErrMsg(format!("parse uri failed, {:?}", e))),
        )
    }

    pub fn get_bytes(
        &mut self,
        key: &str,
    ) -> impl Future<Item = Option<Vec<u8>>, Error = failure::Error> {
        self.inner
            .range(Request::new(RangeRequest {
                key: key.as_bytes().to_vec(),
                ..Default::default()
            }))
            .map_err(|e| panic!("gRPC request failed; err={:?}", e))
            .and_then(|resp| Ok(resp.into_inner().kvs.first().map(|kv| kv.value.to_vec())))
    }

    pub fn get_string(
        &mut self,
        key: &str,
    ) -> impl Future<Item = Option<String>, Error = failure::Error> {
        self.inner
            .range(Request::new(RangeRequest {
                key: key.as_bytes().to_vec(),
                ..Default::default()
            }))
            .map_err(|e| panic!("gRPC request failed; err={:?}", e))
            .and_then(|resp| {
                Ok(resp
                    .into_inner()
                    .kvs
                    .first()
                    .map(|kv| String::from_utf8_lossy(&kv.value).to_string()))
            })
    }
}
