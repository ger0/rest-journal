#![deny(elided_lifetimes_in_paths)]
use actix_web::web::Bytes;
use actix_web::{App, web, HttpResponse, HttpRequest, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{RwLock, Mutex};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use sha256::digest;
use std::time::{SystemTime, Duration};


const TOKEN_LENGTH: usize = 32;

// journal entry
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Journal {
    title:      String,
    data:       String,
    #[serde(skip_serializing, default)]
    etag:       String
}

// task entry
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Task {
    text:       String,
    done:       bool,
    #[serde(skip_serializing, default)]
    etag:       String
}

trait Etagged {
    fn get_etag(&self) -> String;
    fn set_etag(&mut self, etag: String);
}

impl Etagged for Journal {
    fn get_etag(&self) -> String {
        return self.etag.clone();
    }
    fn set_etag(&mut self, etag: String) {
        self.etag = etag;
    }
}

impl Etagged for Task {
    fn get_etag(&self) -> String {
        return self.etag.clone();
    }
    fn set_etag(&mut self, etag: String) {
        self.etag = etag;
    }
}

#[derive(PartialEq)]
struct Token {
    timestamp:  SystemTime,
    value:      String,
}

// Application state
struct State {
    journals:   RwLock<HashMap<usize, Journal>>,
    tasks:      RwLock<HashMap<usize, Task>>,
    tokens:     Mutex<Vec<Token>>
}

trait Readable<T> {
    fn get_hmap(&self) -> &RwLock<HashMap<usize, T>>;
}

impl Readable<Journal> for State {
    fn get_hmap(&self) -> &RwLock<HashMap<usize, Journal>> {
        return &self.journals;
    }
}

impl Readable<Task> for State {
    fn get_hmap(&self) -> &RwLock<HashMap<usize, Task>> {
        return &self.tasks;
    }
}

const VALID_TIME_TOKEN: Duration = Duration::from_secs(60 * 3); 

impl State {
    fn gen_token(&self) -> String {
        let mut tokens  = self.tokens.lock().unwrap();

        // cleaning older tokens...
        let timestamp   =  SystemTime::now();

        // 3 minutes for a token to become invalid
        // removal of invalid entries
        tokens.retain(|item| item.timestamp >= (timestamp - VALID_TIME_TOKEN));

        let rng = thread_rng();
        let str_value: String = rng
            .sample_iter(&Alphanumeric)
            .take(TOKEN_LENGTH)
            .map(char::from)
            .collect();
        let token = Token{
            timestamp,
            value: str_value.clone(),
        };
        tokens.push(token);
        return str_value;
    }

    fn consume_token(&self, token: &str) -> bool {
        let mut tokens = self.tokens.lock().unwrap();
        if let Some(index) = tokens.iter().position(|x| *x.value == *token) {
            let rmv = tokens.remove(index);
            if rmv.timestamp < (SystemTime::now() - VALID_TIME_TOKEN) {
                return false;
            } else {
                return true;
            }
        } else {
            false
        }
    }

    fn rm_resource<T>(&self, id: &usize) -> Result<&str, &str> where State: Readable<T> {
        let hmap: &RwLock<HashMap<usize, T>> = self.get_hmap();
        let mut resources = hmap.write().unwrap();
        if let Some(_) = resources.get(&id) {
            resources.remove(&id);
            return Ok("Removed");
        } else {
            return Err("Not found");
        }
    }

    fn add_resource<T: Etagged + Serialize>(&self, 
        mut resource: T, 
        uri: String
    ) -> Result<String, String> where State: Readable<T> {
        let mut resources = self.get_hmap().write().unwrap();
        let index = resources.len();
        let uri = format!("{}/{}", uri, index);
        let serialized_json = match serde_json::to_string(&resource) {
            Ok(srlz)    => srlz,
            Err(_)      => return Err(String::from("Error during serialization")),
        };
        let etag = calculate_hash(serialized_json);
        resource.set_etag(etag.clone());
        resources.insert(index, resource);
        println!("Resource created {}, added at index: {}", uri, index);
        return Ok(uri);
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

async fn gen_token(state: web::Data<State>) -> impl Responder {
    let token = state.gen_token();
    println!("Generated token: {}", token);
    HttpResponse::Created()
        .body(String::from(token))
}

async fn get_by_id<T: Serialize + Etagged>(
    path: web::Path<usize>,
    state: web::Data<State>,
) -> impl Responder where State: Readable<T>
{
    let id = path.into_inner();

    let hmap: &RwLock<HashMap<usize, T>> = state.get_hmap();
    let resources = hmap.read().unwrap();
    if let Some(resource) = resources.get(&id) {
        let etag = resource.get_etag();
        return HttpResponse::Ok()
            .append_header(("ETag", etag))
            .json(resource);
    } else {
        return HttpResponse::NotFound().body("Not found");
    }
}

#[derive(Serialize, Deserialize)]
struct TaskMerge {
    ids: Vec<usize>
}

fn response_token(
    state: &web::Data<State>,
    request: &HttpRequest
) -> Result<(), HttpResponse> {
    let bad_request = |reason| Err(HttpResponse::BadRequest().body(String::from(reason)));
    let token_val = match request.headers().get("Post-Token") {
        Some(token) => token,
        None        => return bad_request("Missing token"),
    };
    let token = match token_val.to_str() {
        Ok(str) => str,
        Err(_)  => return bad_request("Error during token retrieval"),
    };
    let is_allowed = state.consume_token(token);
    if !is_allowed {
        return bad_request("Bad token");
    }
    return Ok(());
}

async fn merge_tasks(
    json: web::Json<TaskMerge>,
    state: web::Data<State>,
    request: HttpRequest
) -> impl Responder where State: Readable<Task> {
    if let Err(resp) = response_token(&state, &request) {
        return resp;
    }
    let tasks = state.tasks.read().unwrap();
    let info: TaskMerge = json.into_inner();
    let (merged_text, all_done) = info.ids.iter()
        .filter_map(|id| tasks.get(id))
        .fold((String::new(), true), |(mut merged, all_true), item| {
            merged.push('\n');
            merged.push_str(&item.text);
            (merged, all_true && item.done)
        },
    );
    println!("Merged task data: {}", merged_text.clone());

    let new_task = Task {
        text: merged_text,
        done: all_done,
        etag: String::from(""),
    };
    let uri = String::from(request.uri().path());
    drop(tasks);
    let location = match state.add_resource(new_task, uri) {
        Ok(res)  => res,
        Err(res) => return HttpResponse::InternalServerError()
            .body(res)
    };
    // else if it didn't fail, remove old entries
    for id in info.ids {
        state.rm_resource::<Task>(&id).unwrap();
    };
    return HttpResponse::Created()
            .append_header(("Location", location))
            .body(String::from("OK"));
}

async fn post_resource<T: Etagged + Serialize>(
    json: web::Json<T>, 
    state: web::Data<State>, 
    request: HttpRequest
) -> impl Responder where State: Readable<T> {
    if let Err(resp) = response_token(&state, &request) {
        return resp;
    }
    let uri = String::from(request.uri().path());
    let full_uri = match &state.add_resource(json.into_inner(), uri.clone()) {
        Ok(index) => index.clone(),
        Err(text) => return HttpResponse::InternalServerError().body(text.clone())
    };
    return HttpResponse::Created()
        .append_header(("Location", full_uri)).body(String::from("OK"))
}

async fn delete_resource<T>(
    path: web::Path<usize>,
    state: web::Data<State>,
) -> impl Responder where State: Readable<T>, T: Serialize {
    let id = path.into_inner();
    match &state.rm_resource(&id) {
        Ok(msg)  => return HttpResponse::Ok().body(String::from(*msg)),
        Err(msg) => return HttpResponse::NotFound().body(String::from(*msg))
    };
}

fn calculate_hash(json_string: String) -> String {
    let hash = digest(json_string);
    hash 
}

fn check_etag<T: Etagged>(
    resource: &T, 
    request: &HttpRequest) -> Result<(), HttpResponse> {
    let etag = match request.headers().get("If-Match") {
        Some(etag)  => etag,
        None        => return Err(HttpResponse::PreconditionRequired().body("ETag is missing!")),
    };
    let etag = match etag.to_str() {
        Ok(etag)    => etag,
        Err(_)      => return Err(HttpResponse::BadRequest().body("Broken header!")),
    };
    if resource.get_etag() != etag {
        return Err(HttpResponse::PreconditionFailed().body("ETag does not match!"));
    }
    return Ok(());
}

async fn patch_task(
    payload:    Bytes,
    app_state:  web::Data<State>,
    path:       web::Path<usize>,
    request:    HttpRequest,
) -> impl Responder {
    let bad_request = |reason| HttpResponse::BadRequest().body(String::from(reason));

    let id = path.into_inner();
    let mut tasks = app_state.tasks.write().unwrap();
    let mut task = match tasks.get_mut(&id) {
        Some(task)  => task,
        None        => return bad_request("No such resource"),
    };

    if let Err(response) = check_etag(task, &request) {
        return response;
    }

    let json: Value = match serde_json::from_slice(&payload) {
        Ok(json)    => json,
        Err(_)      => return bad_request("Broken json"),
    };

    let mut is_updated = false;
    if let Some(done) = json.get("done") {
        if let Some(done) = done.as_bool() {
            task.done = done;
            is_updated = true;
        }
    }

    if let Some(text) = json.get("text") {
        if let Some(text) = text.as_str() {
            task.text = String::from(text);
            is_updated = true;
        }
    }

    if is_updated {
        let serialized_json = match serde_json::to_string(&json) {
            Ok(srlz)    => srlz,
            Err(_)      => return HttpResponse::BadRequest().body("Json error"),
        };
        let new_etag = calculate_hash(serialized_json);
        task.set_etag(new_etag.clone());
        return HttpResponse::Ok()
            .append_header(("ETag", new_etag))
            .body("Updated");
    } else {
        return bad_request("Nothing to update");
    }
}

async fn put_resource<T>(
    json:       web::Json<T>,
    app_state:  web::Data<State>,
    path:       web::Path<usize>,
    request:    HttpRequest
) -> impl Responder where State: Readable<T>, T: Serialize + Etagged {
    let id = path.into_inner();

    let hmap: &RwLock<HashMap<usize, T>> = app_state.get_hmap();
    let mut resources = hmap.write().unwrap();

    if let Some(resource) = resources.get(&id) {
        if let Err(response) = check_etag(resource, &request) {
            return response;
        }
    }

    // else put the element in the HashMap of the resource
    let serialized_json = match serde_json::to_string(&json.0) {
        Ok(srlz)    => srlz,
        Err(_)      => return HttpResponse::BadRequest().body("json error"),
    };

    let mut new_resource = json.into_inner();
    let new_etag = calculate_hash(serialized_json);
    new_resource.set_etag(new_etag.clone());
    resources.insert(id, new_resource);

    return HttpResponse::Ok()
        .append_header(("ETag", new_etag))
        .body("Updated");
}

async fn get_resources<T>(
    query: web::Query<PaginationParams>,
    app_state: web::Data<State>,
) -> impl Responder where State: Readable<T>, T: Serialize {
    // I'll end up in hell for this...
    let hmap: &RwLock<HashMap<usize, T>> = app_state.get_hmap();
    let resources = hmap.read().unwrap();

    let page_num = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(5);

    let total_entries = resources.len();
    let total_pages = (total_entries + per_page - 1) / per_page;

    let start_index = (page_num - 1) * per_page;

    let ids: Vec<&usize> = resources.keys()
        .into_iter().collect();
    let item_slice: Vec<&T> = ids.into_iter().skip(start_index).take(per_page)
        .map(|id| resources.get(id).unwrap())
        .collect();
    
    let response = PaginationResponse {
        page: page_num,
        total_entries,
        total_pages,
        entries: item_slice.to_vec(),
    };
    
    HttpResponse::Ok().json(response)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let mut tasks: HashMap<usize, Task> = HashMap::new();
    let mut journals: HashMap<usize, Journal> = HashMap::new();
    for i in 0..10 {
        journals.insert(i, Journal{
            title: format!("Title {}", i),
            data: String::from("Hello World!"),
            etag: String::from("1")
        });
        tasks.insert(i, Task{
            text: format!("Do the {}", i),
            done: false,
            etag: String::from("1")
        });
    }
    let app_state = web::Data::new(State {
        journals:   RwLock::new(journals),
        tasks:      RwLock::new(tasks),
        tokens:     Mutex::new(Vec::<Token>::new())
    });

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .service(
                web::resource("/tokens")
                .route(web::post().to(gen_token))
            )
            .service(
                web::resource("/tasks")
                .route(web::get().to(get_resources::<Task>))
                .route(web::post().to(post_resource::<Task>))
            )
            .service(
                web::resource("/tasks/{id}")
                .route(web::get().to(get_by_id::<Task>))
                .route(web::delete().to(delete_resource::<Task>))
                .route(web::put().to(put_resource::<Task>))
                .route(web::patch().to(patch_task))
            )
            .service(
                web::resource("/task_merger")
                .route(web::post().to(merge_tasks))
            )
            .service(
                web::resource("/journals")
                .route(web::get().to(get_resources::<Journal>))
                .route(web::post().to(post_resource::<Journal>))
            )
            .service(
                web::resource("/journals/{id}")
                .route(web::get().to(get_by_id::<Journal>))
                .route(web::delete().to(delete_resource::<Journal>))
                .route(web::put().to(put_resource::<Journal>))
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
