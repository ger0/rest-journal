use actix_web::{App, web, HttpResponse, HttpRequest, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use std::sync::{RwLock, Mutex};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::any::TypeId;

#[derive(Debug, Clone)]
struct Metadata {
    etag:   String,
    id:     usize
}

// journal entry
#[derive(Debug, Clone)]
struct Journal {
    metainf:    Metadata,
    title:      String,
    data:       String,
}

impl Serialize for Journal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        #[derive(Serialize, Deserialize)]
        struct Temp {
            id:     usize,
            title:  String,
            data:   String,
        }
        let temp = Temp {
            id:     self.metainf.id,
            title:  self.title.clone(),
            data:   self.data.clone()
        };
        temp.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Journal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
        let json_obj = serde_json::Value::deserialize(deserializer)?;

        let id_test = json_obj["id"].as_u64();
        let id = id_test.unwrap();

        let title = json_obj["title"]
            .as_str()
            .unwrap();

        let data = json_obj["data"]
            .as_str()
            .unwrap();

        Ok(Journal {
            metainf: Metadata {
                id: id as usize,
                etag: String::from("Test"),
            },
            title:  String::from(title),
            data:   String::from(data)
        })
    }
}

// task entry
#[derive(Debug, Clone)]
struct Task {
    metainf:    Metadata,
    text:       String,
    done:       bool,
}

impl Serialize for Task {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        #[derive(Serialize, Deserialize)]
        struct Temp {
            id:     usize,
            text:   String,
            done:   bool,
        }
        let temp = Temp {
            id:     self.metainf.id,
            text:   self.text.clone(),
            done:   self.done,
        };
        temp.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Task {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
        let json_obj = serde_json::Value::deserialize(deserializer)?;
        let id = json_obj["id"].as_u64().unwrap();
        let text = json_obj["text"]
            .as_str()
            .unwrap();
        let done = json_obj["done"]
            .as_bool()
            .unwrap();
        Ok(Task {
            metainf: Metadata {
                id: id as usize,
                etag: String::from("Test"),
            },
            text: String::from(text),
            done
        })
    }
}
const TOKEN_LENGTH: usize = 16;

// Application state
struct State {
    journals:   RwLock<HashMap<usize, Journal>>,
    tasks:      RwLock<HashMap<usize, Task>>,
    tokens:     Mutex<Vec<String>>
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

async fn gen_token(state: web::Data<State>) -> impl Responder {
    let token = state.gen_token();
    println!("Generated token: {}", token);
    HttpResponse::Created()
        .body(String::from(token))
}

async fn get_by_id<T: 'static>(
    path: web::Path<usize>,
    state: web::Data<State>,
) -> impl Responder
{
    let id = path.into_inner();
    let mut response = HttpResponse::NotFound().body("Not found");
    if TypeId::of::<T>() == TypeId::of::<Journal>() {
        response = HttpResponse::Ok().json(
            state.journals.
                read().
                unwrap()
                .get(&id));
    } else if TypeId::of::<T>() == TypeId::of::<Task>() {
        response = HttpResponse::Ok().json(
            state.tasks
                .read()
                .unwrap()
                .get(&id));
    }
    return response;
}

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
    journals.insert(index, data);
    println!("{}, added at index: {}", journals[&index].data, index);
    return HttpResponse::Created()
        .append_header(("Location", uri)).body(String::from("OK"))
}

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
    tasks.insert(index, data);
    println!("{}, done? {}, added at index: {}", tasks[&index].text, tasks[&index].done, index);
    return HttpResponse::Created()
        .append_header(("Location", uri)).body(String::from("OK"))
}

async fn delete_resource<T>(
    path: web::Path<usize>,
    app_state: web::Data<State>,
) -> impl Responder where State: Readable<T>, T: Serialize {
    let hmap: &RwLock<HashMap<usize, T>> = app_state.get_hmap();
    let mut resources = hmap.write().unwrap();
    let id = path.into_inner();
    if let Some(_) = resources.get(&id) {
        resources.remove(&id);
        HttpResponse::Ok().json("Removed")
    } else {
        HttpResponse::NotFound().body("Not found")
    }
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
    let end_index = start_index + per_page;

    let item_slice: Vec<&T> = resources
        .iter()
        .filter(|(&id, _)| id >= start_index && id <= end_index)
        .map(|(_, entry)| entry)
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
            metainf: Metadata{
                id: i,
                etag: String::from("1")
            },
            title: format!("Title {}", i),
            data: String::from("Hello World!")
        });
        tasks.insert(i, Task{
            metainf: Metadata{
                id: i,
                etag: String::from("1")
            },
            text: format!("Do the {}", i),
            done: false
        });
    }
    let app_state = web::Data::new(State {
        journals:   RwLock::new(journals),
        tasks:      RwLock::new(tasks),
        tokens:     Mutex::new(Vec::<String>::new())
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
                .route(web::post().to(add_task))
            )
            .service(
                web::resource("/tasks/{id}")
                .route(web::get().to(get_by_id::<Task>))
                .route(web::delete().to(delete_resource::<Task>))
            )
            .service(
                web::resource("/journals")
                .route(web::get().to(get_resources::<Journal>))
                .route(web::post().to(add_journal))
            )
            .service(
                web::resource("/journals/{id}")
                .route(web::get().to(get_by_id::<Journal>))
                .route(web::delete().to(delete_resource::<Journal>))
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
