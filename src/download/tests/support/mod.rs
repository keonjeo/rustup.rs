extern crate futures;
extern crate hyper;
extern crate tempdir;

use std::fs::{self, File};
use std::io::{self, Read};
use std::net::SocketAddr;
use std::path::Path;

use self::futures::sync::oneshot;
use self::tempdir::TempDir;

pub fn tmp_dir() -> TempDir {
    TempDir::new("rustup-download-test-").expect("creating tempdir for test")
}

pub fn file_contents(path: &Path) -> String {
    let mut result = String::new();
    File::open(&path)
        .unwrap()
        .read_to_string(&mut result)
        .expect("reading test result file");
    result
}

pub fn write_file(path: &Path, contents: &str) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(path)
        .expect("writing test data");

    io::Write::write_all(&mut file, contents.as_bytes()).expect("writing test data");

    file.sync_data().expect("writing test data");
}

pub fn serve_file(contents: Vec<u8>) -> SocketAddr {
    use self::futures::Future;
    use std::thread;

    let addr = ([127, 0, 0, 1], 0).into();
    let (addr_tx, addr_rx) = oneshot::channel();

    thread::spawn(move || {
        // XXX: multiple clone below
        fn serve(
            contents: Vec<u8>,
        ) -> impl Fn(hyper::Request<hyper::Body>) -> hyper::Response<hyper::Body> {
            move |req| serve_contents(req, contents.clone())
        }

        let server = hyper::server::Server::bind(&addr)
            .serve(move || hyper::service::service_fn_ok(serve(contents.clone())));
        let addr = server.local_addr();
        addr_tx.send(addr).unwrap();
        hyper::rt::run(server.map_err(|e| panic!(e)));
    });
    let addr = addr_rx.wait().unwrap();
    addr
}

fn serve_contents(
    req: hyper::Request<hyper::Body>,
    contents: Vec<u8>,
) -> hyper::Response<hyper::Body> {
    let mut range_header = None;
    let (status, body) = if let Some(range) = req.headers().get(hyper::header::RANGE) {
        // extract range "bytes={start}-"
        let range = range.to_str().expect("unexpected Range header");
        assert!(range.starts_with("bytes="));
        let range = range.trim_left_matches("bytes=");
        assert!(range.ends_with("-"));
        let range = range.trim_right_matches("-");
        assert_eq!(range.split("-").count(), 1);
        let start: u64 = range.parse().expect("unexpected Range header");

        range_header = Some(format!("bytes {}-{len}/{len}", start, len = contents.len()));
        (
            hyper::StatusCode::PARTIAL_CONTENT,
            contents[start as usize..].to_vec(),
        )
    } else {
        (hyper::StatusCode::OK, contents)
    };

    let mut res = hyper::Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_LENGTH, body.len())
        .body(hyper::Body::from(body))
        .unwrap();
    if let Some(range) = range_header {
        res.headers_mut()
            .insert(hyper::header::CONTENT_RANGE, range.parse().unwrap());
    }
    res
}
