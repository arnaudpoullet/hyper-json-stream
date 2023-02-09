use hyper::{Body, Uri};

use futures_util::stream::StreamExt;
use hyper::Client;
use hyper_json_stream::JsonStream;
use hyper_rustls::HttpsConnectorBuilder;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Country {
    name: String,
    country: String,
}

#[tokio::main]
async fn main() {
    let url = "https://raw.githubusercontent.com/lutangar/cities.json/master/cities.json";

    let client = Client::builder().build::<_, Body>(
        HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_only()
            .enable_http1()
            .build(),
    );
    // Fetch the url...
    let res = client.get(Uri::from_static(url));

    let stream: JsonStream<Country> = JsonStream::new(res, 1, 100);

    //Optionally take only a number of elements from the list
    let mut stream = stream.take(10);

    while let Some(country_result) = stream.next().await {
        let country = country_result.unwrap();
        println!("{} {}", country.name, country.country);
    }
}
