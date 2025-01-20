use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    widgets::Paragraph,
    DefaultTerminal,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io,
    net::{Ipv4Addr, SocketAddrV4},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use warp::Filter;

const ADDRESS: [u8; 4] = [127, 0, 0, 1];
const PORT: u16 = 33433;

const TIMESTAMP: &str = "@timestamp";
const AGENT_ID: &str = "agent.id";
const HOST_NAME: &str = "host.name";
const HOST_OS_NAME: &str = "host.os.name";
const USER_NAME: &str = "user.name";
const HOST_IP: &str = "host.ip";

type JsonValue = serde_json::Value;
type JsonMap = HashMap<String, JsonValue>;
type SharedAppState = Arc<Mutex<AppState>>;
type TerminalBackend = ratatui::Terminal<ratatui::prelude::CrosstermBackend<io::Stdout>>;

#[derive(Serialize, Deserialize, Debug)]
struct Log {
    values: Vec<Vec<JsonValue>>, // A 2D vector holding the log values
    took: u32,                   // Time taken to process the log
    columns: Vec<Column>,        // Metadata about the columns in the log
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
    name: String, // Name of the column
    #[serde(rename = "type")]
    column_type: String, // Type of the column, renamed to "type" in JSON
}

#[derive(Serialize, Deserialize, Debug)]
struct AppState {
    current_document: Log,    // The current log document
    mapped_document: JsonMap, // A map of column names to their values
}

impl AppState {
    fn new() -> SharedAppState {
        Arc::new(Mutex::new(Self {
            current_document: Log::new(),
            mapped_document: HashMap::new(),
        }))
    }

    // Update the current log and map the document
    fn update_log(&mut self, new_log: Log) {
        self.current_document = new_log;
        self.mapped_document = HashMap::new();

        // Map the columns to their respective values
        for (i, column) in self.current_document.columns.iter().enumerate() {
            if let Some(value) = self.current_document.values[0].get(i) {
                self.mapped_document
                    .insert(column.name.clone(), value.clone());
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize the terminal
    let mut terminal = ratatui::init();
    terminal.clear().unwrap();

    // Run the application
    if let Err(e) = run(terminal) {
        panic!("error in rendering thread: {:?}", e);
    }

    // Restore the terminal state
    ratatui::restore();
}

fn run(terminal: DefaultTerminal) -> io::Result<()> {
    // Create the application state
    let app_state = AppState::new();

    // Spawn the server thread
    tokio::spawn(server_thread(app_state.clone()));

    // Spawn the drawing thread
    thread::spawn(draw_thread(terminal, app_state.clone()));

    // Handle user input
    take_input()?;
    Ok(())
}

// The draw_thread function is responsible for rendering the UI.
// It takes a terminal and a shared application state as arguments.
// The function returns a closure that will be executed in a separate thread.
// Inside the closure, it calls the draw_ui function to update the terminal with the current state.
// If an error occurs during the UI drawing process, it will be printed to the standard error output.

fn draw_thread(terminal: TerminalBackend, app_state_draw: SharedAppState) -> impl FnOnce() {
    move || {
        if let Err(e) = draw_ui(terminal, app_state_draw) {
            eprintln!("Error in draw_ui: {:?}", e);
        }
    }
}

// The server_thread function is responsible for handling incoming HTTP requests.
// It takes a shared application state as an argument and runs an asynchronous server using Warp.
// The function defines a route for receiving logs via a POST request to the "/data" path.
// When a log is received, it updates the application state with the new log and responds with the current document.
// The server listens on the specified address and port, and runs indefinitely until the application is terminated.

async fn server_thread(app_state_server: SharedAppState) {
    // Define the route for receiving logs
    let logs_route = warp::post()
        .and(warp::path("data"))
        .and(warp::body::json())
        .map(move |log: Log| {
            let mut state = app_state_server.lock().unwrap();
            state.update_log(log);
            warp::reply::json(&state.current_document)
        });

    // Start the server
    let address = SocketAddrV4::new(Ipv4Addr::from(ADDRESS), PORT);
    warp::serve(logs_route).run(address).await;
}

// The take_input function is responsible for handling user input in a loop.
// It continuously reads events from the terminal and checks for key presses.
// If the 'q' key is pressed, the function breaks out of the loop and returns,
// effectively allowing the user to exit the application.
// The function returns a Result<(), io::Error> to handle any potential I/O errors
// that may occur during the event reading process.

fn take_input() -> Result<(), io::Error> {
    loop {
        // Read user input
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                // Exit the loop if 'q' is pressed
                if let KeyCode::Char('q') = key.code {
                    break;
                }
            }
        }
    }
    Ok(())
}

// The draw_ui function is responsible for rendering the user interface in a loop.
// It takes a terminal and a shared application state as arguments.
// Inside the loop, it sleeps for a short duration before redrawing the UI to avoid excessive CPU usage.
// The function locks the application state to access the mapped document and formats the keys to display.
// It creates a Paragraph widget with the formatted message and renders it on the terminal frame.
// If an error occurs during the drawing process, it will be propagated as an io::Result error.

fn draw_ui(mut terminal: DefaultTerminal, app_state: SharedAppState) -> io::Result<()> {
    loop {
        // Sleep for a short duration before redrawing
        thread::sleep(Duration::from_millis(2500));

        // Draw the UI
        terminal
            .draw(|frame| {
                let map = { &app_state.lock().unwrap().mapped_document };

                // Define the keys to display
                let keys: Vec<&str> = vec![
                    TIMESTAMP,
                    AGENT_ID,
                    HOST_NAME,
                    HOST_OS_NAME,
                    USER_NAME,
                    HOST_IP,
                ];

                // Format the message to display
                let message = keys
                    .iter()
                    .map(|item| format_by_key(item, map))
                    .collect::<String>();

                // Create and render the widget
                let widget = Paragraph::new(format!("{message}"));
                frame.render_widget(widget, frame.area());
            })
            .map(|_| ())?;
    }
}

// This function takes a key and a reference to a JSON map (JsonMap).
// It attempts to retrieve the value associated with the given key from the map.
// If the key exists in the map, it serializes the value to a pretty-printed JSON string.
// The function then formats the key and the serialized value into a string and returns it.
// If the key does not exist in the map, it returns a string indicating that the key is unknown.

fn format_by_key(key: &str, map: &JsonMap) -> String {
    match map.get(key) {
        Some(value) => match serde_json::to_string_pretty(value) {
            Ok(text) => format!("\"{key}\": {text}\n"),
            Err(e) => panic!("error deserializing log: {:?}", e),
        },
        None => format!("\"{key}\": unknown\n"),
    }
}
