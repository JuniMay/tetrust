#![recursion_limit = "256"]

mod game;
mod render;

use crate::game::Game;
use crate::render::Renderer;

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use web_sys::{Document, HtmlElement, KeyboardEvent, PointerEvent, Window};

const DAS_MS: f64 = 140.0; // Delayed Auto Shift
const ARR_MS: f64 = 40.0;  // Auto Repeat Rate

#[derive(Default)]
struct InputState {
    left: bool,
    right: bool,
    down: bool,

    rotate_cw: bool,   // edge-triggered
    rotate_ccw: bool,  // edge-triggered
    hard_drop: bool,   // edge-triggered
    hold: bool,        // edge-triggered
    pause: bool,       // edge-triggered
    reset: bool,       // edge-triggered

    horiz_dir: i32, // -1, 0, +1
    das: f64,
    arr: f64,
}

impl InputState {
    fn clear_edges(&mut self) {
        self.rotate_cw = false;
        self.rotate_ccw = false;
        self.hard_drop = false;
        self.hold = false;
        self.pause = false;
        self.reset = false;
    }
}

struct App {
    renderer: Renderer,
    game: Game,
    input: InputState,

    last_time: Option<f64>,
    overlay_hint: HtmlElement,
}

impl App {
    fn new(window: Window, document: Document) -> Result<Self, JsValue> {
        let renderer = Renderer::new(&document)?;
        renderer.resize_all()?;

        let seed = window
            .performance()
            .map(|p| p.now() as u32)
            .unwrap_or_else(|| js_sys::Date::now() as u32);

        let mut game = Game::new(seed);
        game.reset();

        let overlay_hint = document
            .get_element_by_id("overlay-hint")
            .ok_or_else(|| JsValue::from_str("Missing #overlay-hint"))?
            .dyn_into::<HtmlElement>()?;

        Ok(Self {
            renderer,
            game,
            input: InputState::default(),
            last_time: None,
            overlay_hint,
        })
    }

    fn hide_overlay_hint(&self) {
        let _ = self.overlay_hint.style().set_property("display", "none");
    }

    fn on_key_down(&mut self, e: KeyboardEvent) {
        // Prevent scrolling / page actions for keys we handle.
        match e.key().as_str() {
            "ArrowLeft" | "ArrowRight" | "ArrowDown" | "ArrowUp" | " " => e.prevent_default(),
            _ => {}
        }

        self.hide_overlay_hint();

        match e.code().as_str() {
            "ArrowLeft" => self.input.left = true,
            "ArrowRight" => self.input.right = true,
            "ArrowDown" => self.input.down = true,

            // Rotations
            "ArrowUp" | "KeyX" => {
                if !e.repeat() {
                    self.input.rotate_cw = true;
                }
            }
            "KeyZ" => {
                if !e.repeat() {
                    self.input.rotate_ccw = true;
                }
            }

            // Actions
            "Space" => {
                if !e.repeat() {
                    self.input.hard_drop = true;
                }
            }
            "KeyC" | "ShiftLeft" | "ShiftRight" => {
                if !e.repeat() {
                    self.input.hold = true;
                }
            }
            "KeyP" => {
                if !e.repeat() {
                    self.input.pause = true;
                }
            }
            "KeyR" => {
                if !e.repeat() {
                    self.input.reset = true;
                }
            }
            _ => {}
        }
    }

    fn on_key_up(&mut self, e: KeyboardEvent) {
        match e.code().as_str() {
            "ArrowLeft" => self.input.left = false,
            "ArrowRight" => self.input.right = false,
            "ArrowDown" => self.input.down = false,
            _ => {}
        }
    }

    fn on_button_down(&mut self, id: &str, e: PointerEvent) {
        e.prevent_default();
        self.hide_overlay_hint();

        match id {
            "btn-left" => self.input.left = true,
            "btn-right" => self.input.right = true,
            "btn-down" => self.input.down = true,

            "btn-rot-cw" => self.input.rotate_cw = true,
            "btn-rot-ccw" => self.input.rotate_ccw = true,
            "btn-drop" => self.input.hard_drop = true,
            "btn-hold" => self.input.hold = true,

            "btn-pause" => self.input.pause = true,
            "btn-reset" => self.input.reset = true,

            _ => {}
        }
    }

    fn on_button_up(&mut self, id: &str, e: PointerEvent) {
        e.prevent_default();

        match id {
            "btn-left" => self.input.left = false,
            "btn-right" => self.input.right = false,
            "btn-down" => self.input.down = false,
            _ => {}
        }
    }

    fn handle_horizontal(&mut self, dt_ms: f64) {
        // Resolve direction.
        let dir = match (self.input.left, self.input.right) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };

        if dir == 0 {
            self.input.horiz_dir = 0;
            self.input.das = 0.0;
            self.input.arr = 0.0;
            return;
        }

        // New press or direction change => move immediately and reset timers.
        if dir != self.input.horiz_dir {
            self.input.horiz_dir = dir;
            self.input.das = 0.0;
            self.input.arr = 0.0;
            self.game.move_horiz(dir);
            return;
        }

        // Held: DAS then ARR.
        self.input.das += dt_ms;
        if self.input.das < DAS_MS {
            return;
        }

        self.input.arr += dt_ms;
        while self.input.arr >= ARR_MS {
            self.input.arr -= ARR_MS;
            self.game.move_horiz(dir);
        }
    }

    fn frame(&mut self, time_ms: f64) -> Result<(), JsValue> {
        let dt = match self.last_time {
            None => {
                self.last_time = Some(time_ms);
                0.0
            }
            Some(last) => {
                self.last_time = Some(time_ms);
                (time_ms - last).clamp(0.0, 50.0) // clamp for tab-switch / background throttling
            }
        };

        // Edge-triggered UI actions
        if self.input.reset {
            self.game.reset();
        }
        if self.input.pause {
            self.game.toggle_pause();
        }

        // If paused/game-over, still render but suppress movement.
        if !self.game.is_paused() && !self.game.is_game_over() {
            self.handle_horizontal(dt);

            if self.input.rotate_cw {
                self.game.rotate(true);
            } else if self.input.rotate_ccw {
                self.game.rotate(false);
            }

            if self.input.hold {
                self.game.hold();
            }
            if self.input.hard_drop {
                self.game.hard_drop();
            }

            self.game.tick(dt, self.input.down);
        }

        self.renderer.render(&self.game)?;

        // Clear edge-triggered inputs after consuming them.
        self.input.clear_edges();

        Ok(())
    }
}

fn window() -> Result<Window, JsValue> {
    web_sys::window().ok_or_else(|| JsValue::from_str("No global window"))
}

fn document(window: &Window) -> Result<Document, JsValue> {
    window
        .document()
        .ok_or_else(|| JsValue::from_str("No document on window"))
}

fn bind_button(app: Rc<RefCell<App>>, doc: &Document, id: &str) -> Result<(), JsValue> {
    let el = doc
        .get_element_by_id(id)
        .ok_or_else(|| JsValue::from_str(&format!("Missing #{id}")))?;

    // pointerdown
    {
        let id = id.to_string();
        let app = app.clone();
        let cb = Closure::<dyn FnMut(PointerEvent)>::wrap(Box::new(move |e: PointerEvent| {
            app.borrow_mut().on_button_down(&id, e);
        }));
        el.add_event_listener_with_callback("pointerdown", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // pointerup
    {
        let id = id.to_string();
        let app = app.clone();
        let cb = Closure::<dyn FnMut(PointerEvent)>::wrap(Box::new(move |e: PointerEvent| {
            app.borrow_mut().on_button_up(&id, e);
        }));
        el.add_event_listener_with_callback("pointerup", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // pointercancel
    {
        let id = id.to_string();
        let app = app.clone();
        let cb = Closure::<dyn FnMut(PointerEvent)>::wrap(Box::new(move |e: PointerEvent| {
            app.borrow_mut().on_button_up(&id, e);
        }));
        el.add_event_listener_with_callback("pointercancel", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    Ok(())
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let win = window()?;
    let doc = document(&win)?;

    let app = Rc::new(RefCell::new(App::new(win.clone(), doc.clone())?));

    // Keyboard listeners
    {
        let app = app.clone();
        let cb = Closure::<dyn FnMut(KeyboardEvent)>::wrap(Box::new(move |e: KeyboardEvent| {
            app.borrow_mut().on_key_down(e);
        }));
        win.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }
    {
        let app = app.clone();
        let cb = Closure::<dyn FnMut(KeyboardEvent)>::wrap(Box::new(move |e: KeyboardEvent| {
            app.borrow_mut().on_key_up(e);
        }));
        win.add_event_listener_with_callback("keyup", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // Resize listener
    {
        let app = app.clone();
        let cb = Closure::<dyn FnMut()>::wrap(Box::new(move || {
            let _ = app.borrow().renderer.resize_all();
        }));
        win.add_event_listener_with_callback("resize", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // Buttons (touch controls)
    for id in [
        "btn-left",
        "btn-right",
        "btn-down",
        "btn-rot-cw",
        "btn-rot-ccw",
        "btn-drop",
        "btn-hold",
        "btn-pause",
        "btn-reset",
    ] {
        bind_button(app.clone(), &doc, id)?;
    }

    // RAF loop
    let f = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64)>>));
    let g = f.clone();

    let app_for_frame = app.clone();
    *g.borrow_mut() = Some(Closure::<dyn FnMut(f64)>::wrap(Box::new(move |t: f64| {
        {
            let _ = app_for_frame.borrow_mut().frame(t);
        }
        // schedule next frame
        let win = web_sys::window().unwrap();
        let cb = f.borrow();
        let cb = cb.as_ref().unwrap();
        let _ = win.request_animation_frame(cb.as_ref().unchecked_ref());
    })));

    // Kick off
    let win = web_sys::window().unwrap();
    let cb = g.borrow();
    let cb = cb.as_ref().unwrap();
    win.request_animation_frame(cb.as_ref().unchecked_ref())?;

    Ok(())
}

fn main() {}
