use actix_multipart::Multipart;
use actix_web::{
    error, get,
    http::{header, Uri},
    post, put, web, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use futures_util::stream::StreamExt as _;
use rand::{distributions::Alphanumeric, Rng};
use std::path::{Path, PathBuf};
use tokio::{fs::File, io::AsyncWriteExt};

/// Returns the value of the Host header from a HttpRequest.
fn get_host_header(req: HttpRequest) -> Result<String, actix_web::Error> {
    req.headers()
        .get(header::HOST)
        .ok_or(error::ErrorBadRequest("Host header not present"))?
        .to_str()
        .map(|e| e.into())
        .map_err(|e| {
            error::ErrorInternalServerError(format!("Failed to parse Host header string: {:?}", e))
        })
}

/// Generates a random 4-character file ID.
fn create_file_id() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(4)
        .map(char::from)
        .collect()
}

/// Generates a full URL for a file, given its ID and filename.
fn build_file_url(req: HttpRequest, id: &str, filename: &str) -> Result<String, actix_web::Error> {
    let host = get_host_header(req)?;
    Ok(Uri::builder()
        .scheme("https")
        .authority(host)
        .path_and_query(format!("/{}/{}", id, filename))
        .build()?
        .to_string())
}

fn build_local_path(id: &str, filename: &str) -> PathBuf {
    let ext: &str = match Path::new(&filename).extension() {
        Some(ext) => ext.to_str().unwrap_or("bin"),
        None => "bin",
    };
    Path::new("uploads").join(&id).with_extension(ext)
}

#[post("/upload")]
async fn form_upload(
    mut payload: Multipart,
    req: HttpRequest,
) -> actix_web::Result<impl Responder, actix_web::Error> {
    if let Some(field) = payload.next().await {
        let mut field = field?;

        // Get filename
        let filename = if field.name() == "text" {
            "paste.txt"
        } else {
            field
                .content_disposition()
                .get_filename()
                .unwrap_or("file.bin")
        }
        .to_string();

        // Create a unique ID
        let id = create_file_id();

        // Open the file
        let dest_path = build_local_path(&id, &filename);
        let mut file = File::create(dest_path).await?;

        // Read the chunks
        while let Some(chunk) = field.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
        }
        file.flush().await?;

        // Return the file URL
        let url = build_file_url(req, &id, &filename)?;
        Ok(HttpResponse::Ok().body(url))
    } else {
        Ok(HttpResponse::BadRequest().body("At least one attachment is required"))
    }
}

#[put("/{filename}")]
async fn put_file(
    filename: web::Path<String>,
    body: web::Bytes,
    req: HttpRequest,
) -> actix_web::Result<impl Responder, actix_web::Error> {
    // Create a unique ID
    let id = create_file_id();

    // Build the filenames
    let filename = filename.into_inner();
    let dest_path = build_local_path(&id, &filename);

    // Create a file
    let mut file = File::create(dest_path).await?;
    file.write_all(&body).await?;
    file.flush().await?;

    // Build the URL
    let url = build_file_url(req, &id, &filename)?;
    Ok(HttpResponse::Ok().body(url))
}

async fn get_file(id_ext: &str) -> actix_web::Result<HttpResponse, actix_web::Error> {
    // Get the local filename by appending the extension to the id
    let local_file = Path::new("uploads").join(id_ext);

    // Get the file
    let file = match File::open(&local_file).await {
        Ok(file) => file,
        Err(_) => return Ok(HttpResponse::NotFound().body("404 Not Found")),
    };

    // Build the response
    let mut response = HttpResponse::Ok();

    // Guess the mime type
    if let Some(mime) = mime_guess::from_path(local_file).first() {
        response.insert_header(header::ContentType(mime));
    }

    // Return the file stream
    let stream = tokio_util::io::ReaderStream::new(file);
    Ok(response.streaming(stream))
}

#[get("/{id}/{filename}")]
async fn get_file_with_filename(
    path: web::Path<(String, String)>,
) -> actix_web::Result<HttpResponse, actix_web::Error> {
    let (id, filename) = path.into_inner();

    // Get the local filename by appending the extension to the id
    let mut id_ext = Path::new(&id).to_owned();
    if let Some(ext) = Path::new(&filename).extension() {
        id_ext.set_extension(ext);
    }
    let id_ext = id_ext
        .to_str()
        .ok_or(error::ErrorInternalServerError("Internal server error"))?;

    get_file(id_ext).await
}

#[get("/{id_ext}")]
async fn get_file_without_filename(
    path: web::Path<String>,
) -> actix_web::Result<HttpResponse, actix_web::Error> {
    let id_ext = path.into_inner();
    get_file(&id_ext).await
}

#[get("/")]
async fn index() -> actix_web::Result<impl Responder, actix_web::Error> {
    let index_file = include_str!("../static/index.html");
    return Ok(HttpResponse::Ok()
        .insert_header(header::ContentType::html())
        .body(index_file));
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(index)
            .service(get_file_without_filename)
            .service(get_file_with_filename)
            .service(put_file)
            .service(form_upload)
    })
    .bind(("127.0.0.1", 1337))?
    .run()
    .await
}
