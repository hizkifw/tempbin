use actix_files::Files;
use actix_web::{get, http::header, post, put, web, App, HttpResponse, HttpServer, Responder};
use rand::{distributions::Alphanumeric, Rng};
use std::path::Path;
use tokio::{fs::File, io::AsyncWriteExt};

#[post("/upload")]
async fn form_upload() -> impl Responder {
    HttpResponse::Ok().body("ok")
}

#[put("/{filename}")]
async fn put_file(
    filename: web::Path<String>,
    body: web::Bytes,
) -> actix_web::Result<impl Responder, actix_web::Error> {
    // Create a unique ID
    let id: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(4)
        .map(char::from)
        .collect();

    // Build the filenames
    let filename = filename.into_inner();
    let mut dest_path = Path::new("uploads").join(&id);
    if let Some(ext) = Path::new(&filename).extension() {
        dest_path.set_extension(ext);
    }

    // Create a file
    let mut file = File::create(dest_path).await?;
    file.write_all(&body).await?;
    file.flush().await?;

    Ok(HttpResponse::Ok().body(format!("{}/{}", id, filename)))
}

#[get("/{id}/{filename}")]
async fn get_file(
    path: web::Path<(String, String)>,
) -> actix_web::Result<impl Responder, actix_web::Error> {
    let (id, filename) = path.into_inner();

    // Get the local filename by appending the extension to the id
    let mut local_file = Path::new("uploads").join(id);
    if let Some(ext) = Path::new(&filename).extension() {
        local_file.set_extension(ext);
    }

    // Get the file
    let file = match File::open(local_file).await {
        Ok(file) => file,
        Err(_) => return Ok(HttpResponse::NotFound().body("404 Not Found")),
    };

    // Build the response
    let mut response = HttpResponse::Ok();

    // Guess the mime type
    if let Some(mime) = mime_guess::from_path(&filename).first() {
        response.insert_header(header::ContentType(mime));
    }

    // Return the file stream
    let stream = tokio_util::io::ReaderStream::new(file);
    Ok(response.streaming(stream))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new().service(get_file).service(put_file).service(
            Files::new("/", "./static")
                .prefer_utf8(true)
                .index_file("index.html"),
        )
    })
    .bind(("127.0.0.1", 1337))?
    .run()
    .await
}
