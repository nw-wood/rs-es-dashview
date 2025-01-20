//imports
use std::{collections::HashMap, io, sync::{Arc, Mutex}, thread, time::Duration};
use ratatui::{crossterm::event::{self, Event, KeyCode, KeyEventKind}, widgets::Paragraph, DefaultTerminal};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use warp::Filter;

#[derive(Serialize, Deserialize, Debug)]
struct Log {
    values: Vec<Vec<serde_json::Value>>, //enum of json types provided by serde_json crate
    took: u32,
    columns: Vec<Column>,
}

impl Log {
    fn new() -> Self {
        Self {
            values: vec![vec![]],
            took: 0,
            columns: vec![],
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Column {
    name: String,
    #[serde(rename = "type")] //swaps the column_type with type when deserializing columns
    column_type: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct AppState {
    current_document: Log, //the custom log structure thats been built
    mapped_document: HashMap<String, serde_json::Value> //HashMap<column name, associated log value>
}

impl AppState {
    //stick this thing into an arc mutex to access between threads
    fn new() -> Arc<Mutex<Self>> {
        Arc::new(
            Mutex::new(
                Self {
                    current_document: Log::new(),
                    mapped_document: HashMap::new(),
                }
            )
        )
    }

    //when the server gets documents on the data endpoint from logstash a hash map gets built out of the document
    fn update_log(&mut self, new_log: Log) {

        self.current_document = new_log;
        self.mapped_document = HashMap::new();

        self.current_document.columns
            .iter()
            .enumerate()
            .for_each(|(i, column)| {
                self.mapped_document.insert(column.name.clone(), self.current_document.values[0][i].clone());
        });
    }
}

#[tokio::main]
async fn main() {

    //prepare ratatui which is a terminal rendering library
    let mut terminal = ratatui::init();

    //clear the terminal out for rendering
    if let Err(e) =  terminal.clear() {
        panic!("error when clearing the terminal: {:?}", e);
    }

    //start running the application
    if let Err(e) = run(terminal) {
        panic!("error in rendering thread: {:?}", e);
    }

    //restore the terminal back to normal behavior
    ratatui::restore();
}

fn run(terminal: DefaultTerminal) -> io::Result<()> {

    //create the application state, and some references to it to pass into threads
    let app_state = AppState::new();
    let app_state_server = app_state.clone();
    let app_state_draw = app_state.clone();

    //spawn the server with a reference to the application state
    let _server = tokio::spawn(async move {
        //create a warp filter that translates to a json receiving endpoint
        let logs_route = warp::post()
            .and(warp::path("data"))
            .and(warp::body::json())
            .map(move |log: Log| {
                //when documents are recieved on the endpoint update the app state
                let mut state = app_state_server.lock().unwrap();
                state.update_log(log);
                warp::reply::json(&state.current_document)
            });
        //start up the server
        warp::serve(logs_route).run(([127, 0, 0, 1], 33433)).await;
    });

    //spawn the drawing thread for rendering to the terminal output
    thread::spawn(|| -> io::Result<()> {
        draw_ui(terminal, app_state_draw)?;
        Ok(())
    });

    //use this thread (main) to take key input while the drawing thread and server thread are doing work
    loop {
        if let Event::Key(key) = event::read()? {
            match key.kind {
                KeyEventKind::Press => {
                    match key.code {
                        KeyCode::Char('q') => {
                            break;
                        },
                        _ => {}
                    }
                }
                _ => {},
            }
        }
    }
    Ok(())
}

//the draw thread
fn draw_ui(mut terminal: DefaultTerminal, app_state: Arc<Mutex<AppState>>) -> io::Result<()> {
    loop {
        //lazily slow this down - realistically this is only ever updating every 10 seconds 
        thread::sleep(Duration::from_millis(2500));
        terminal.draw(|frame| {

            //pull the current document hashmap from the application state
            let map = { &app_state.lock().unwrap().mapped_document};
            //some keys to try indexing from the map
            let keys: Vec<String> = vec![
                "@timestamp".to_string(),
                "agent.id".to_string(),
                "host.name".to_string(),
                "host.os.name".to_string(),
                "user.name".to_string(),
                "host.ip".to_string(),
            ];

            //a message built using the keys and a helper function
            let message= keys
                .iter()
                .map(|item| format_by_key(item.clone(), map))
                .collect::<Vec<String>>().join("");

            //create a simple ratatui out of the message that got created
            let widget = Paragraph::new(format!("{message}"));
            //render it to the terminal
            frame.render_widget(widget, frame.area());

        })
        //map the result to a unit struct, and if there's a problem in this whole process just prop back the error
        .map(|_| ())?;
    }
}

//message building helper function
fn format_by_key(key: String, map: &HashMap<String, Value>) -> String {
    let key_value = map.get(&key);
    //if the key returns a result then we'll return it back formatted and with a line break
    if let Some(value) = key_value {
        match serde_json::to_string_pretty(value) {
            Ok(text) => format!("\"{key}\": {text}\n"),
            Err(e) => panic!("error deserializing log: {:?}", e),
        }
    //in the event the key isn't found in the hashmap then just display unknown for that key
    } else {
        format!("\"{key}\": unknown\n")
    }
}