use actix_web::{get, post, App, web, HttpResponse, HttpRequest, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use std::sync::{RwLock, Mutex};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

// journal entry
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Journal {
    title: String,
    data: String,
}

// Todo entry
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Task {
    text: String,
    done: bool
}
const TOKEN_LENGTH: usize = 16;

// Application state
struct State {
    journals:   RwLock<Vec<Journal>>,
    tasks:      RwLock<Vec<Task>>,
    tokens:     Mutex<Vec<String>>
}

impl State {
    fn gen_token(&self) -> String {
        let mut tokens = self.tokens.lock().unwrap();
        let rng = thread_rng();
        let token: String = rng
            .sample_iter(&Alphanumeric)
            .take(TOKEN_LENGTH)
            .map(char::from)
            .collect();
        tokens.push(token.clone());
        token
    }
    fn consume_token(&self, token: &str) -> bool {
        let mut tokens = self.tokens.lock().unwrap();
        if let Some(index) = tokens.iter().position(|x| *x == token) {
            tokens.remove(index);
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Deserialize)]
struct PaginationParams {
    page: Option<usize>,
    per_page: Option<usize>,
}

#[derive(Debug, Serialize)]
struct PaginationResponse<T> {
    page: usize,
    total_entries: usize,
    total_pages: usize,
    entries: Vec<T>,
}
// ------------------------------------ TOKENS ----------------------------------------
#[post("/tokens")]
async fn gen_token(state: web::Data<State>) -> impl Responder {
    let token = state.gen_token();
    println!("Generated token: {}", token);
    HttpResponse::Created()
        .body(String::from(token))
}

// ------------------------------------ JOURNALS --------------------------------------
#[get("/journals")]
async fn get_journals(
    query: web::Query<PaginationParams>,
    app_state: web::Data<State>,
) -> impl Responder {
    let page_num = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(5);

    let journals = app_state.journals.read().unwrap();

    let total_entries = journals.len();
    let total_pages = (total_entries + per_page - 1) / per_page;

    let start_index = (page_num - 1) * per_page;
    let end_index = start_index + per_page;
    let paginated_journals = &journals[start_index..end_index];

    let response = PaginationResponse {
        page: page_num,
        total_entries,
        total_pages,
        entries: paginated_journals.to_vec(),
    };

    HttpResponse::Ok().json(response)
}

#[get("/journals/{id}")]
async fn get_journals_by_id(
    path: web::Path<usize>,
    app_state: web::Data<State>,
) -> impl Responder {

    let journal_id = path.into_inner();
    println!("Path: {}", journal_id);
    if let Some(journal) = app_state.journals.read().unwrap().get(journal_id) {
        HttpResponse::Ok().json(journal)
    } else {
        HttpResponse::NotFound().body("Journal not found")
    }
}

#[post("/journals")]
async fn add_journal(json: web::Json<Journal>, state: web::Data<State>, request: HttpRequest) -> impl Responder {
    let token_val = request.headers().get("Post-Token");
    if token_val == None {
        return HttpResponse::BadRequest()
            .body(String::from("Missing token"))
    }
    let token = token_val.unwrap().to_str().unwrap();
    let is_allowed = state.consume_token(token);

    if !is_allowed {
        return HttpResponse::BadRequest()
            .body(String::from("Bad token"))

    }
    let mut journals = state.journals.write().unwrap();
    let index = journals.len();

    let uri = format!("{}/{}", request.uri().path(), index);

    let data = json.into_inner();
    journals.push(data);
    println!("{}, added at index: {}", journals[index].data, index);
    return HttpResponse::Created()
        .append_header(("Location", uri)).body(String::from("OK"))
}

// ------------------------------------ TASKS -----------------------------------------
#[post("/tasks")]
async fn add_task(json: web::Json<Task>, state: web::Data<State>, request: HttpRequest) -> impl Responder {
    let token_val = request.headers().get("Post-Token");
    if token_val == None {
        return HttpResponse::BadRequest()
            .body(String::from("Missing token"))
    }
    let token = token_val.unwrap().to_str().unwrap();
    let is_allowed = state.consume_token(token);

    if !is_allowed {
        return HttpResponse::BadRequest()
            .body(String::from("Bad token"))

    }
    let mut tasks = state.tasks.write().unwrap();
    let index = tasks.len();

    let uri = format!("{}/{}", request.uri().path(), index);

    let data = json.into_inner();
    tasks.push(data);
    println!("{}, done? {}, added at index: {}", tasks[index].text, tasks[index].done, index);
    return HttpResponse::Created()
        .append_header(("Location", uri)).body(String::from("OK"))
}

#[get("/tasks/{id}")]
async fn get_tasks_by_id(
    path: web::Path<usize>,
    app_state: web::Data<State>,
) -> impl Responder {

    let id = path.into_inner();
    if let Some(task) = app_state.journals.read().unwrap().get(id) {
        HttpResponse::Ok().json(task)
    } else {
        HttpResponse::NotFound().body("Journal not found")
    }
}

#[get("/tasks")]
async fn get_tasks(
    query: web::Query<PaginationParams>,
    app_state: web::Data<State>,
) -> impl Responder {
    let page_num = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(5);

    let tasks = app_state.tasks.read().unwrap();

    let total_entries = tasks.len();
    let total_pages = (total_entries + per_page - 1) / per_page;

    let start_index = (page_num - 1) * per_page;
    let end_index = start_index + per_page;
    let paginated_tasks = &tasks[start_index..end_index];

    let response = PaginationResponse {
        page: page_num,
        total_entries,
        total_pages,
        entries: paginated_tasks.to_vec(),
    };

    HttpResponse::Ok().json(response)
}

// ------------------------------------ MAIN  -----------------------------------------

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let mut tasks: Vec<Task> = Vec::new();
    let mut journals: Vec<Journal> = Vec::new();
    for i in 0..10 {
        journals.push(Journal{
            title: format!("Title {}", i),
            data: String::from("Hello World!")
        });
        tasks.push(Task{
            text: format!("Do the {}", i),
            done: false
        })
    }
    let app_state = web::Data::new(State {
        journals:   RwLock::new(journals),
        tasks:      RwLock::new(tasks),
        tokens:     Mutex::new(Vec::<String>::new())
    });

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .service(gen_token)
            .service(get_journals_by_id)
            .service(get_journals)
            .service(add_journal)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
