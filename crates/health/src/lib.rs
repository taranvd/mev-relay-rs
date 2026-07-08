use warp::Filter;

pub async fn serve_health(port: u16) {
    let route =
        warp::path("health").map(|| warp::reply::with_status("ok", warp::http::StatusCode::OK));

    warp::serve(route).run(([0, 0, 0, 0], port)).await;
}
