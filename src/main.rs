#[macro_use]
extern crate log;

use actix_files::NamedFile;
use actix_multipart::Multipart;
use actix_web::{
    error, get,
    http::{header, Uri},
    post, put, web, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use futures_util::stream::StreamExt as _;
use lazy_static::lazy_static;
use rand::{distributions::Alphanumeric, Rng};
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};
use tokio::{fs::File, io::AsyncWriteExt, time::Duration};

const FILE_SIZE_LIMIT: usize = 2_000_000_000;
const UPLOADS_FOLDER: &str = "uploads";
const INDEX_FILE: &str = include_str!("./index.html");
lazy_static! {
    static ref PURGE_AFTER: Duration = Duration::from_secs(3600 * 24);
}

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
        .path_and_query(format!(
            "/{}/{}{}",
            urlencoding::encode(id),
            urlencoding::encode(filename),
            if !filename.contains(".") { ".bin" } else { "" }
        ))
        .build()?
        .to_string())
}

fn build_local_path(id: &str, filename: &str) -> PathBuf {
    let ext: &str = match Path::new(&filename).extension() {
        Some(ext) => ext.to_str().unwrap_or("bin"),
        None => "bin",
    };
    Path::new(UPLOADS_FOLDER).join(&id).with_extension(ext)
}

#[post("/upload")]
async fn form_upload(
    mut payload: Multipart,
    req: HttpRequest,
) -> actix_web::Result<impl Responder, actix_web::Error> {
    while let Some(field) = payload.next().await {
        let mut field = field?;

        // Skip if file is empty
        if field.name() == "file"
            && field
                .content_disposition()
                .get_filename()
                .unwrap_or("")
                .is_empty()
        {
            continue;
        }

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
        return Ok(HttpResponse::Ok().body(url));
    }

    Ok(HttpResponse::BadRequest().body("At least one attachment is required"))
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

/// Returns the file as a streaming response
async fn get_file(id_ext: &str) -> impl Responder {
    let local_file = Path::new(UPLOADS_FOLDER).join(id_ext);
    NamedFile::open_async(local_file).await
}

#[get("/{id}/{filename}")]
async fn get_file_with_filename(
    path: web::Path<(String, String)>,
) -> actix_web::Result<impl Responder> {
    let (id, filename) = path.into_inner();

    // Get the local filename by appending the extension to the id
    let mut id_ext = Path::new(&id).to_owned();
    if let Some(ext) = Path::new(&filename).extension() {
        id_ext.set_extension(ext);
    }
    let id_ext = id_ext
        .to_str()
        .ok_or(error::ErrorInternalServerError("Internal server error"))?;

    Ok(get_file(id_ext).await)
}

#[get("/{id_ext}")]
async fn get_file_without_filename(path: web::Path<String>) -> impl Responder {
    let id_ext = path.into_inner();
    get_file(&id_ext).await
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok()
        .insert_header(header::ContentType::html())
        .body(INDEX_FILE)
}

/// Loops through all the files in the uploads directory and deletes files older
/// than PURGE_AFTER.
async fn purge() -> anyhow::Result<()> {
    // Loop through all the files in the folder
    let mut dir = tokio::fs::read_dir(UPLOADS_FOLDER).await?;
    while let Ok(Some(entry)) = dir.next_entry().await {
        let created_at = entry
            .metadata()
            .await
            .expect("failed to get metadata")
            .modified()
            .expect("failed to get created time");

        // Check if file is older than threshold
        let dur = SystemTime::now()
            .duration_since(created_at)
            .unwrap_or(Duration::from_secs(0));
        if dur > *PURGE_AFTER {
            // Delete
            if let Err(e) = tokio::fs::remove_file(entry.path()).await {
                error!(
                    "Error deleting file {}: {:?}",
                    entry.path().to_string_lossy(),
                    e
                );
            } else {
                info!("Deleted file {}", entry.path().to_string_lossy());
            }
        }
    }

    Ok(())
}

async fn purge_loop(mut rx_stop: tokio::sync::oneshot::Receiver<()>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(e) = purge().await {
                    error!("Error purging files: {:?}", e);
                }
            },
            _ = &mut rx_stop => break,
        }
    }
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    // Set up logger
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    // Get environment variables
    let bind = std::env::var("LISTEN").unwrap_or("127.0.0.1:1337".into());

    // Set up purger loop
    let (tx_stop, stop_loop) = tokio::sync::oneshot::channel();
    let t_loop = tokio::spawn(purge_loop(stop_loop));

    // Start the HTTP server
    info!("Listening on {}", bind);
    HttpServer::new(|| {
        App::new()
            .app_data(web::PayloadConfig::new(FILE_SIZE_LIMIT))
            .wrap(actix_web::middleware::Logger::default())
            .service(index)
            .service(get_file_without_filename)
            .service(get_file_with_filename)
            .service(put_file)
            .service(form_upload)
    })
    .bind(bind)?
    .run()
    .await?;

    // Send the stop signal and wait
    info!("Shutting down...");
    tx_stop.send(()).unwrap();
    t_loop.await?;

    Ok(())
}
