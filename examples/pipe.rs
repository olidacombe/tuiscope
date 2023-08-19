/// This example is intended to a act a bit like [fzf](https://github.com/junegunn/fzf),
/// where stdin provides the options, and the user types to filter, using up/down to
/// select a choice.  Then pressing Enter prints the choice to stdout and the program
/// exits.
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event::Key, EventStream, KeyCode, KeyEvent,
        KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use eyre::Result;
use futures::StreamExt;
use std::{io, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::{sync::mpsc::channel, time::interval};
use tokio::{sync::mpsc::Sender, task::JoinHandle};
use tracing::error;
use tui::{prelude::*, widgets::*};
use tui_input::{backend::crossterm::EventHandler, Input};
use tuiscope::{FuzzyFinder, FuzzyList};

#[derive(Default)]
enum AppState {
    #[default]
    Reading,
    Ready,
}

/// App holds the state of the application
#[derive(Default)]
struct App<'a> {
    /// Current value of the input box
    input: Input,
    /// Options to fuzzy select from
    options: Vec<String>,
    /// Fuzzy Finder
    pub fuzzy_finder: FuzzyFinder<'a>,
    state: AppState,
}

impl<'a> App<'a> {
    pub fn push_option(&'a mut self, option: String) {
        self.options.push(option);
        self.fuzzy_finder.add_options(&*self.options);
    }
    pub fn selection(&self) -> Result<String> {
        Ok(self
            .fuzzy_finder
            .selection()
            .map(|s| s.value.to_string())
            .unwrap_or_default())
    }
    pub fn select_next(&mut self) {
        self.fuzzy_finder.select_next();
    }
    pub fn select_prev(&mut self) {
        self.fuzzy_finder.select_prev();
    }
    pub fn handle_key(&mut self, key: &crossterm::event::Event) {
        self.input.handle_event(key);
        self.fuzzy_finder.set_filter(self.input.to_string());
    }
}

enum Event {
    // When `stdio` is exhausted
    EOF,
    Key(KeyEvent),
    NewLine(String),
    Tick,
}

async fn tick_task(tx: Sender<Event>) -> Result<JoinHandle<()>> {
    let mut interval = interval(Duration::from_millis(100));
    Ok(tokio::spawn(async move {
        loop {
            interval.tick().await;
            if let Err(_) = tx.send(Event::Tick).await {
                break;
            }
        }
    }))
}

async fn crossterm_event_task(tx: Sender<Event>) -> Result<JoinHandle<()>> {
    let mut events = EventStream::new();
    Ok(tokio::spawn(async move {
        loop {
            if let Some(Ok(Key(event))) = events.next().await {
                if let Err(_) = tx.send(Event::Key(event)).await {
                    continue;
                }
            }
        }
    }))
}

async fn stdin_task(tx: Sender<Event>) -> Result<JoinHandle<()>> {
    let mut input_lines = BufReader::new(tokio::io::stdin()).lines();
    Ok(tokio::spawn(async move {
        while let Some(line) = input_lines.next_line().await.unwrap() {
            if let Err(e) = tx.send(Event::NewLine(line)).await {
                error!("{e:?}");
            }
        }
        tx.send(Event::EOF).await.ok();
    }))
}

#[tokio::main]
async fn main() -> Result<()> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let res = run_app(&mut terminal).await;

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    match res {
        Err(err) => println!("{err:?}"),
        Ok(res) => println!("{res}"),
    }

    Ok(())
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> Result<String> {
    let mut app = App::default();
    // let mut options = Vec::<String>::new(); // immutable frozen from `elsa` may work

    let (tx, mut rx) = channel::<Event>(20);
    tick_task(tx.clone()).await?;
    crossterm_event_task(tx.clone()).await?;
    stdin_task(tx).await?;

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Some(event) = rx.recv().await {
            match event {
                Event::EOF => {
                    app.state = AppState::Ready;
                }
                Event::NewLine(line) => {
                    // options.push(line);
                    // app.push_option(line);
                    // app.fuzzy_finder.add_options(&options);
                }
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Enter => {
                                return app.selection();
                            }
                            KeyCode::Up => {
                                app.select_prev();
                            }
                            KeyCode::Down => {
                                app.select_next();
                            }
                            KeyCode::Esc => return Ok(String::default()),
                            _ => {
                                app.handle_key(&crossterm::event::Event::Key(key));
                            }
                        }
                    }
                }
                Event::Tick => {}
            }
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)].as_ref())
        .split(f.size());

    let input = Paragraph::new(app.input.to_string())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Filter"));
    f.render_widget(input, chunks[0]);

    let results_title = match app.state {
        AppState::Reading => "[Loading] Options",
        AppState::Ready => "Options",
    };
    let fuzzy_results = FuzzyList::default()
        .matched_char_style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL).title(results_title))
        .selection_highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_stateful_widget(fuzzy_results, chunks[1], &mut app.fuzzy_finder);
}
