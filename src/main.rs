use warp::Filter;

mod rss;

#[tokio::main]
async fn main() {
    println!("Rust server started");

    let hello = warp::get()
        .and(warp::path!("hello" / String))
        .map(|name| format!("Hello, {}!", name));

    let rss = warp::get()
        .and(warp::path!("rss"))
        .and(warp::query::<rss::RawQuery>())
        .and_then(rss::handler)
        .recover(rss::handle_error);

    let routes = hello.or(rss);

    warp::serve(routes).run(([0, 0, 0, 0], 8080)).await;
}
