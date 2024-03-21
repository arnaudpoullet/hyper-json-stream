use http::Method;
use hyper::Request;
use hyper::Uri;

use futures_util::stream::StreamExt;
use http_body_util::Empty;
use hyper::body::Bytes;
use hyper_json_stream::JsonStream;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Country {
    name: String,
    country: String,
}

#[tokio::main]
async fn main() {
    let url = "https://raw.githubusercontent.com/lutangar/cities.json/master/cities.json";

    let client = Client::builder(TokioExecutor::new()).build::<_, Empty<Bytes>>(
        HttpsConnectorBuilder::new()
            .with_native_roots()
            .unwrap()
            .https_only()
            .enable_http2()
            .build(),
    );
    let req = Request::builder()
        .method(Method::GET)
        .uri(Uri::from_static(url))
        .header("Accept-Encoding", "gzip")
        .body(Empty::new())
        .unwrap();
    let res = client.request(req);

    let stream: JsonStream<Country> = JsonStream::new(res, 1, 100);

    // Optionally take only a number of elements from the list
    let mut stream = stream.take(10);

    while let Some(country_res) = stream.next().await {
        let country = country_res.unwrap();
        println!("{} {}", country.name, country.country);
    }
}
