#![allow(unused)]
use std::{cell::{Cell, RefCell}, rc::Rc, time::{Duration, Instant}};

use color_eyre::eyre::Result;
use crossterm::{event::{self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event}, execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}};
use ratatui::DefaultTerminal;
use spin_sleep::sleep_until;
use scopeguard::defer;

use crate::{context::Context};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExitRequest {
    Success,
    Failure(i32),
}

pub struct LoopContext {
    exit_request: RefCell<Option<ExitRequest>>,
    redraw_requested: RefCell<Option<()>>,
}

impl LoopContext {
    fn new() -> Self {
        Self {
            exit_request: RefCell::new(None),
            redraw_requested: RefCell::new(None),
        }
    }
    
    fn take_request(&self) -> Option<ExitRequest> {
        self.exit_request.borrow_mut().take()
    }
    
    fn take_redraw_request(&self) -> bool {
        self.redraw_requested.borrow_mut().take().is_some()
    }
    
    pub fn request_exit(&self, request: ExitRequest) {
        *self.exit_request.borrow_mut() = Some(request);
    }
    
    pub fn request_render(&self) {
        self.redraw_requested.borrow_mut().replace(());
    }
}

pub struct CancellableExitRequest {
    pub request: ExitRequest,
    cancel: Rc<Cell<bool>>,
}

impl CancellableExitRequest {
    pub(crate) fn new(request: ExitRequest, cancel: Rc<Cell<bool>>) -> Self {
        Self {
            request,
            cancel,
        }
    }
    
    pub fn cancel(&self) {
        self.cancel.set(true);
    }
}

pub enum GameEvent<'a> {
    TermEvent(Event),
    Begin(&'a GameSettings),
    Update,
    Render,
    ExitRequested(CancellableExitRequest),
    Exiting,
}

pub struct GameSettings {
    pub render_frametime: Duration,
    pub update_frametime: Duration,
}

pub trait EventHandler<Marker> {
    type Error;
    fn handle_event(&mut self, terminal: &mut DefaultTerminal, event: GameEvent, context: &LoopContext) -> Result<(), Self::Error>;
    
    #[allow(unused)]
    fn error_filter(&mut self, error: Result<(), Self::Error>, context: &LoopContext) -> Result<(), Self::Error> {
        error
    }
}

impl<E, F> EventHandler<(E, F)> for F
where
    F: FnMut(&mut DefaultTerminal, GameEvent, &LoopContext) -> Result<(), E>
{
    type Error = E;
    fn handle_event(&mut self, terminal: &mut DefaultTerminal, event: GameEvent, context: &LoopContext) -> Result<(), Self::Error> {
        (self)(terminal, event, context)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoopError<E> {
    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("{0}")]
    UserError(E),
}

struct PastaReader {
    buffer: String,
}

enum PastaEvent {
    Pasta(String),
    PastaWithEvent(String, Event),
}

impl PastaReader {
    pub fn new() -> Self {
        Self {
            buffer: String::with_capacity(1024*64),
        }
    }
    
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
    
    pub fn read_pastes(&mut self, first_paste: String) -> Result<PastaEvent, std::io::Error> {
        self.clear();
        self.buffer.push_str(&first_paste);
        loop {
            if !event::poll(Duration::ZERO)? {
                let result = Ok(PastaEvent::Pasta(self.buffer.clone()));
                self.buffer.clear();
                return result;
            }
            match event::read()? {
                Event::Paste(pasta) => self.buffer.push_str(&pasta),
                event => {
                    let result = Ok(PastaEvent::PastaWithEvent(self.buffer.clone(), event));
                    self.buffer.clear();
                    return result;
                }
            }
        }
    }
}

pub fn run<M, H: EventHandler<M>>(settings: GameSettings, mut event_handler: H) -> Result<ExitRequest, H::Error> {
    let mut terminal = ratatui::init();
    enable_raw_mode().expect("Failed to enable raw mode.");
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
    ).unwrap();
    let loop_context = LoopContext::new();
    let mut next_update_time = Instant::now();
    let mut next_render_time = Instant::now();
    let mut pasta_reader = PastaReader::new();
    macro_rules! event {
        ($event:expr) => {
            {
                let result = event_handler.handle_event(&mut terminal, $event, &loop_context);
                event_handler.error_filter(result, &loop_context)
            }
        };
    }
    event!(GameEvent::Begin(&settings))?;
    let exit_request = 'game_loop: loop {
        loop {
            if event::poll(Duration::ZERO).expect("Failed to poll.") {
                match event::read().expect("Failed to read event.") {
                    // Event::Paste(pasta) => {
                    //     match pasta_reader.read_pastes(pasta).expect("Failed on paste.") {
                    //         PastaEvent::Pasta(pasta) => {
                    //             event!(GameEvent::TermEvent(Event::Paste(pasta)))?;
                    //         }
                    //         PastaEvent::PastaWithEvent(pasta, event) => {
                    //             event!(GameEvent::TermEvent(Event::Paste(pasta)))?;
                    //             event!(GameEvent::TermEvent(event))?;
                    //         }
                    //     }
                    // }
                    event => {
                        event!(GameEvent::TermEvent(event))?;
                    }
                }
            } else {
                if pasta_reader.buffer.len() != 0 {
                    event!(GameEvent::TermEvent(Event::Paste(pasta_reader.buffer.clone())));
                    pasta_reader.clear();
                }
                break;
            }
        }
        let current_time = Instant::now();
        if next_update_time <= current_time {
            next_update_time += settings.update_frametime;
            event!(GameEvent::Update)?;
        }
        if next_render_time <= current_time {
            event!(GameEvent::Render)?;
            next_render_time += settings.render_frametime;
            loop_context.take_redraw_request();
        } else if loop_context.take_redraw_request() {
            event!(GameEvent::Render)?;
        }
        if let Some(request) = loop_context.take_request() {
            let cancel = Rc::new(Cell::new(false));
            let cancellable = CancellableExitRequest::new(request, Rc::clone(&cancel));
            event!(GameEvent::ExitRequested(cancellable))?;
            if !cancel.get() {
                event!(GameEvent::Exiting)?;
                break 'game_loop request;
            }
        }
    };
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen,
    );
    disable_raw_mode().expect("Failed to disable raw mode.");
    ratatui::restore();
    Ok(exit_request)
}