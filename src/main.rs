// main.rs

use failure::{err_msg, Error};
use futures::Future;
use hyper::client::connect::{Destination, HttpConnector};
use tower_grpc::Request;
use tower_hyper::client::ConnectError;
use tower_hyper::{client, util};
use tower_util::MakeService;

use crate::etcdserverpb::RangeRequest;

use crate::etcdserverpb::client::Kv;

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

fn main() {
    let _ = ::env_logger::init();

    let ip = "127.0.0.1";
    let port = 2379;

    let uri: http::Uri = format!("http://{}:{}", ip, port).parse().unwrap();

    let dst = Destination::try_from_uri(uri.clone()).unwrap();
    let connector = util::Connector::new(HttpConnector::new(4));
    let settings = client::Builder::new().http2_only(true).clone();
    let mut make_client = client::Connect::new(connector, settings);

    let say_hello = make_client
        .make_service(dst)
        .map(move |conn| {
            use etcdserverpb::client::Kv;

            let conn = tower_request_modifier::Builder::new()
                .set_origin(uri)
                .build(conn)
                .unwrap();

            Kv::new(conn)
        })
        .and_then(|mut client| {
            use etcdserverpb::RangeRequest;

            client
                .range(Request::new(RangeRequest {
                    key: "hello".as_bytes().to_vec(),
                    ..Default::default()
                }))
                .map_err(|e| panic!("gRPC request failed; err={:?}", e))
        })
        .and_then(|response| {
            println!("RESPONSE = {:?}", response);
            Ok(())
        })
        .map_err(|e| {
            println!("ERR = {:?}", e);
        });

    tokio::run(say_hello);

    let run = KvClient::new(ip, port)
        .map_err(|e| println!("ERR = {:?}", e))
        .and_then(move |mut client| {
            client
                .get_string("hello")
                .map(|resp| (client, resp))
                .map_err(|e| println!("ERR = {:?}", e))
        })
        .and_then(|(client, resp)| {
            println!("resp=> {:?}", resp);
            Ok(client)
        })
        .and_then(|client| Ok(()))
        .map_err(|e| println!("ERR = {:?}", e));

    tokio::run(run);

    let run = KvClient::new(ip, port)
        .map_err(|e| println!("ERR = {:?}", e))
        .and_then(move |mut client| {
            client
                .get_string("hello")
                .map_err(|e| println!("ERR = {:?}", e))
                .and_then(move |resp| {
                    println!("resp=> {:?}", resp);
                    client
                        .get_string("hello")
                        .map(|_| ())
                        .map_err(|e| println!("ERR = {:?}", e))
                })
        });
    tokio::run(run);
}

struct KvClient {
    inner: Kv<HTTPConn>,
}

impl KvClient {
    pub fn new(
        ip: &str,
        port: u16,
    ) -> impl Future<Item = KvClient, Error = ConnectError<std::io::Error>> {
        let uri: http::Uri = format!("http://{}:{}", ip, port).parse().unwrap();

        let dst = Destination::try_from_uri(uri.clone()).unwrap();
        let connector = util::Connector::new(HttpConnector::new(4));
        let settings = client::Builder::new().http2_only(true).clone();
        let mut make_client = client::Connect::new(connector, settings);

        make_client.make_service(dst).map(move |conn| {
            let conn = tower_request_modifier::Builder::new()
                .set_origin(uri)
                .build(conn)
                .unwrap();

            KvClient {
                inner: Kv::new(conn),
            }
        })
    }

    pub fn get_bytes(&mut self, key: &str) -> impl Future<Item = Option<Vec<u8>>, Error = Error> {
        self.inner
            .range(Request::new(RangeRequest {
                key: key.as_bytes().to_vec(),
                ..Default::default()
            }))
            .map_err(|e| panic!("gRPC request failed; err={:?}", e))
            .and_then(|resp| Ok(resp.into_inner().kvs.first().map(|kv| kv.value.to_vec())))
    }

    pub fn get_string(&mut self, key: &str) -> impl Future<Item = Option<String>, Error = Error> {
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
