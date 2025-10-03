#![allow(unused)]
use std::{cell::{Cell, RefCell}, rc::Rc, time::{Duration, Instant}};

use color_eyre::eyre::Result;
use crossterm::{event::{self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event}, execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}};
use ratatui::DefaultTerminal;
use spin_sleep::sleep_until;
use scopeguard::defer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExitRequest {
    Success,
    Failure(i32),
}

pub struct Context {
    exit_request: RefCell<Option<ExitRequest>>,
    // This seems weird, but `Option<()>` is essentially just a boolean. By doing it this way, I can use `Option::take` to clear the `true` value.
    redraw_request: RefCell<Option<()>>,
    update_request: RefCell<Option<()>>,
}

impl Context {
    fn new() -> Self {
        Self {
            exit_request: RefCell::new(None),
            redraw_request: RefCell::new(None),
            update_request: RefCell::new(None),
        }
    }
    
    fn take_exit_request(&self) -> Option<ExitRequest> {
        self.exit_request.borrow_mut().take()
    }
    
    fn take_redraw_request(&self) -> bool {
        self.redraw_request.borrow_mut().take().is_some()
    }
    
    fn take_update_request(&self) -> bool {
        self.update_request.borrow_mut().take().is_some()
    }
    
    pub fn request_exit(&self, request: ExitRequest) {
        *self.exit_request.borrow_mut() = Some(request);
    }
    
    pub fn request_redraw(&self) {
        self.redraw_request.borrow_mut().replace(());
    }
    
    pub fn request_update(&self) {
        self.update_request.borrow_mut().replace(());
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

pub enum AppEvent<'a> {
    TermEvent(Event),
    Begin(&'a AppSettings),
    Update,
    Render,
    ExitRequested(CancellableExitRequest),
    Exiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameRate {
    OnDemand,
    Duration(Duration),
}

pub struct AppSettings {
    pub render_framerate: FrameRate,
    pub update_framerate: FrameRate,
}

pub trait EventHandler<Marker> {
    type Error;
    fn handle_event(&mut self, terminal: &mut DefaultTerminal, event: AppEvent, context: &Context) -> Result<(), Self::Error>;
    
    #[allow(unused)]
    fn error_filter(&mut self, error: Result<(), Self::Error>, context: &Context) -> Result<(), Self::Error> {
        error
    }
}

impl<E, F> EventHandler<(E, F)> for F
where
    F: FnMut(&mut DefaultTerminal, AppEvent, &Context) -> Result<(), E>
{
    type Error = E;
    fn handle_event(&mut self, terminal: &mut DefaultTerminal, event: AppEvent, context: &Context) -> Result<(), Self::Error> {
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

struct FrameTimer {
    frame_rate: FrameRate,
    next_frame: Instant,
}

impl FrameTimer {
    pub fn new(frame_rate: FrameRate) -> Self {
        Self {
            frame_rate,
            next_frame: Instant::now(),
        }
    }
    
    pub fn frame_ready(&mut self, now: Instant) -> bool {
        match self.frame_rate {
            FrameRate::OnDemand => false,
            FrameRate::Duration(time) => {
                if now >= self.next_frame {
                    self.next_frame = now + time;
                    true
                } else {
                    false
                }
            }
        }
    }
}

pub fn run<M, H: EventHandler<M>>(settings: AppSettings, mut event_handler: H) -> Result<ExitRequest, H::Error> {
    let mut terminal = ratatui::init();
    enable_raw_mode().expect("Failed to enable raw mode.");
    macro_rules! execute {
        ($($tokens:tt)*) => {
            crossterm::execute!(terminal.backend_mut(), $($tokens)*)
        };
    }
    execute!(
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
    ).unwrap();
    let loop_context = Context::new();
    let mut update_timer = FrameTimer::new(settings.update_framerate);
    let mut render_timer = FrameTimer::new(settings.render_framerate);
    macro_rules! event {
        ($event:expr) => {
            {
                let result = event_handler.handle_event(&mut terminal, $event, &loop_context);
                event_handler.error_filter(result, &loop_context)
            }
        };
    }
    event!(AppEvent::Begin(&settings))?;
    let exit_request = 'game_loop: loop {
        loop {
            if !event::poll(Duration::ZERO).expect("Failed to poll.") {
                break;
            }
            match event::read().expect("Failed to read event.") {
                event => {
                    event!(AppEvent::TermEvent(event))?;
                }
            }
        }
        let current_time = Instant::now();
        if update_timer.frame_ready(current_time) {
            event!(AppEvent::Update)?;
            loop_context.take_update_request();
        } else if loop_context.take_update_request() {
            event!(AppEvent::Update)?;
        }
        if render_timer.frame_ready(current_time) {
            event!(AppEvent::Render)?;
            loop_context.take_redraw_request();
        } else if loop_context.take_redraw_request() {
            event!(AppEvent::Render)?;
        }
        if let Some(request) = loop_context.take_exit_request() {
            let cancel = Rc::new(Cell::new(false));
            let cancellable = CancellableExitRequest::new(request, Rc::clone(&cancel));
            event!(AppEvent::ExitRequested(cancellable))?;
            if !cancel.get() {
                event!(AppEvent::Exiting)?;
                break 'game_loop request;
            }
        }
    };
    execute!(
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen,
    );
    disable_raw_mode().expect("Failed to disable raw mode.");
    ratatui::restore();
    Ok(exit_request)
}