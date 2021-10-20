# Hyper JSON Stream

## Deserialize an array of jsons asynchronously with hyper

The code in this library comes from the [backblaze-b2-rs](https://github.com/Darksonn/backblaze-b2-rs/tree/ver0.2/src/b2_future) repo.

This library allows you to consume a  [`hyper::client::ResponseFuture`](https://docs.rs/hyper/0.14.13/hyper/client/struct.ResponseFuture.html) asynchronously while deserializing each element from a json array along the way.

The deserializing itself is done by the `serde_json` crate and is therefore not asynchronous.

This library allows you to process the elements of a big json without having to take everything into memory first and without blocking the entire thread.

## Usage
 Add the following dependencies to your Cargo.toml:
```
hyper = { version = "0.14.13", features = ["client","http2"] }
hyper-rustls = "0.22.1"
serde = { version = "1.0.130", features = ["derive"] }
futures-util = "0.3.17"
```
Create a stream you can iterate on:
```
let hyper_response_future = make_http_request();
let level = 1;
let capacity = 100;
let stream: JsonStream<T> = JsonStream::new(hyper_response_future, level, capacity);
```

The `capacity` sets the initial size of the buffer that will handle the response.

The `level` sets the number of braces (`[` or `{`) to skip before reaching the elements you wish to deserialize.

To deserialize the "Shop" struct in the next example use `level = 2`

```
{
  "shops": [
    {
      "shop_id": 2322,
      "shop_name": "Shop1",
      "shop_state": "OPEN",
    },
    {
      "shop_id": 2422,
      "shop_name": "Shop2",
      "shop_state": "OPEN",
    },
    {
      "shop_id": 2021,
      "shop_name": "Shop3",
      "shop_state": "OPEN",
    },
    {
      "shop_id": 2022,
      "shop_name": "Shop4",
      "shop_state": "OPEN",
    },
  ]
}
```

## Example

Check out [Countries](examples/countries.rs) for a working example.
