mod models;
mod routes;

use axum::{routing::get, Router};

#[shuttle_runtime::main]
async fn axum() -> shuttle_axum::ShuttleAxum {
    let audio_routes = Router::new().route(
        "/",
        get(routes::audios::all_audios).post(routes::audios::new_audio),
    );

    let user_routes = Router::new().route("/:id", get(routes::users::get_user));

    let api_routes = Router::new()
        .nest("/audios", audio_routes)
        .nest("/users", user_routes);

    let app = Router::new().nest("/api", api_routes);

    Ok(app.into())
}
