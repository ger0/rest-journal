use actix_web::{get, post, App, web, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

// journal entry
#[derive(Debug, Serialize, Deserialize)]
#[derive(Clone)]
struct Entry {
    title: String,
    data: String,
}

// Todo entry
#[derive(Debug, Serialize, Deserialize)]
struct Task {
    text: String,
    done: bool
}

struct State {
    journals: Vec<Entry>,
    tasks: Vec<Task>
}

#[derive(Debug, Deserialize)]
struct PaginationParams {
    page: Option<usize>,
    per_page: Option<usize>,
}

#[derive(Debug, Serialize)]
struct PaginationResponse {
    page: usize,
    per_page: usize,
    total_entries: usize,
    total_pages: usize,
    journals: Vec<Entry>,
}

#[get("/journals")]
async fn get_journals(
    query: web::Query<PaginationParams>,
    app_state: web::Data<Mutex<State>>,
) -> impl Responder {
    let page_num = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(5);

    let state = app_state.lock().unwrap();

    let total_entries = state.journals.len();
    let total_pages = (total_entries + per_page - 1) / per_page;

    let start_index = (page_num - 1) * per_page;
    let end_index = start_index + per_page;
    let paginated_journals = &state.journals[start_index..end_index];

    let response = PaginationResponse {
        page: page_num,
        per_page,
        total_entries,
        total_pages,
        journals: paginated_journals.to_vec(),
    };

    HttpResponse::Ok().json(response)
}

#[get("/journals/{id}")]
async fn get_journals_by_id(
    path: web::Path<usize>,
    app_state: web::Data<Mutex<State>>,
) -> impl Responder {

    println!("HELLO!");
    let journal_id = path.into_inner();
    println!("Path: {}", journal_id);
    if let Some(journal) = app_state.lock().unwrap().journals.get(journal_id) {
        HttpResponse::Ok().json(journal)
    } else {
        HttpResponse::NotFound().body("Journal not found")
    }
}

#[post("/journals")]
async fn add_journal(
    req_body: String, 
    app_state: web::Data<Mutex<State>>
) -> impl Responder {

    let mut journal = app_state.lock().unwrap();

    let index = journal.journals.len();
    let uri = format!("/journals/{}", index);

    journal.journals.push(Entry{
        title: String::from("Entry"),
        data: String::from(req_body)
    });
    println!("{}, at index: {}", journal.journals[index].data, index);
    HttpResponse::Created()
        .append_header(("Location", uri)).body(String::from("OK"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let tasks: Vec<Task> = Vec::new();
    let mut journals: Vec<Entry> = Vec::new();
    for i in 0..10 {
        journals.push(Entry{
            title: format!("Title {}", i),
            data: String::from("Hello World!")
        });
    }
    let app_state = web::Data::new(Mutex::new(State {
        journals,
        tasks
    }));

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .service(get_journals_by_id)
            .service(get_journals)
            .service(add_journal)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
